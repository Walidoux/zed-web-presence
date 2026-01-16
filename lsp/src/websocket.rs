/*
 * This file is part of presence-websocket. Extension for Zed that sends presence data to a WebSocket server.
 *
 * Copyright (c) 2024
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>
 */

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, error, info, instrument, warn};
use url::Url;

use crate::activity::ActivityFields;
use crate::document::Document;
use crate::error::Result;

/// Maximum number of connection retries
const MAX_RETRIES: u32 = 5;
/// Initial delay between retries in milliseconds
const INITIAL_DELAY_MS: u64 = 500;
/// Maximum delay between retries in milliseconds
const MAX_DELAY_MS: u64 = 10_000;

#[derive(Debug)]
pub struct WebSocketClient {
    url: String,
    stream: Option<Mutex<futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
        >,
        Message
    >>>,
    connected: Arc<AtomicBool>,
}

impl WebSocketClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            stream: None,
            connected: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns whether the WebSocket client is currently connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    #[instrument(skip(self))]
    pub async fn connect(&mut self) -> Result<()> {
        info!("Connecting to WebSocket server: {}", self.url);

        let url = Url::parse(&self.url).map_err(|e| {
            crate::error::PresenceError::WebSocket(format!("Invalid WebSocket URL: {e}"))
        })?;

        let (ws_stream, _) = connect_async(url).await.map_err(|e| {
            error!("Failed to connect to WebSocket: {}", e);
            crate::error::PresenceError::WebSocket(format!("Failed to connect to WebSocket: {e}"))
        })?;

        let (write, _) = ws_stream.split();
        self.stream = Some(Mutex::new(write));
        self.connected.store(true, Ordering::SeqCst);

        info!("Successfully connected to WebSocket server");
        Ok(())
    }

    /// Connects to WebSocket with exponential backoff retry.
    /// Will retry up to `MAX_RETRIES` times with increasing delays.
    #[instrument(skip(self))]
    pub async fn connect_with_retry(&mut self) -> Result<()> {
        use tokio::time::{Duration, sleep};

        let mut delay = Duration::from_millis(INITIAL_DELAY_MS);

        for attempt in 1..=MAX_RETRIES {
            match self.connect().await {
                Ok(()) => {
                    info!("Connected to WebSocket on attempt {}", attempt);
                    return Ok(());
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        warn!(
                            "Connection attempt {}/{} failed: {}. Retrying in {:?}...",
                            attempt, MAX_RETRIES, e, delay
                        );
                        sleep(delay).await;
                        delay = (delay * 2).min(Duration::from_millis(MAX_DELAY_MS));
                    } else {
                        error!(
                            "Failed to connect to WebSocket after {} attempts: {}",
                            MAX_RETRIES, e
                        );
                        return Err(e);
                    }
                }
            }
        }

        Err(crate::error::PresenceError::WebSocket(
            "Failed to connect after all retries".into(),
        ))
    }

    /// Attempts to reconnect to WebSocket, closing any existing connection first.
    #[instrument(skip(self))]
    pub async fn reconnect(&mut self) -> Result<()> {
        info!("Attempting to reconnect to WebSocket");
        self.connected.store(false, Ordering::SeqCst);

        // Close existing connection
        self.kill().await;

        self.connect_with_retry().await
    }

    pub async fn kill(&mut self) {
        debug!("Closing WebSocket connection");
        self.connected.store(false, Ordering::SeqCst);

        if let Some(stream) = &self.stream {
            let mut stream = stream.lock().await;
            let _ = stream.close().await;
        }
        self.stream = None;
    }

    pub async fn get_stream(&self) -> Result<MutexGuard<'_, futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>
        >,
        Message
    >>> {
        let stream = self.stream.as_ref().ok_or_else(|| {
            crate::error::PresenceError::WebSocket("WebSocket stream not initialized".to_string())
        })?;

        Ok(stream.lock().await)
    }

    #[instrument(skip(self, activity_fields), fields(
            state = activity_fields.state.as_deref().unwrap_or("None"),
            details = activity_fields.details.as_deref().unwrap_or("None")
    ))]
    pub async fn send_presence_data(
        &self,
        activity_fields: ActivityFields,
        doc: Option<&Document>,
        git_remote_url: Option<String>,
    ) -> Result<()> {
        let mut stream = self.get_stream().await?;

        // Extract real filename and extension from Document
        let (file_name, extension) = if let Some(doc) = doc {
            (
                doc.get_filename().unwrap_or_else(|_| "Unknown".to_string()),
                doc.get_extension().to_string(),
            )
        } else {
            (
                activity_fields.details.unwrap_or_else(|| "Unknown".to_string()),
                "".to_string(),
            )
        };

        let presence_data = serde_json::json!({
            "event": "presence",
            "presence": {
                "fileName": file_name,
                "extension": extension,
                "timeSpent": 0, // We'll track this in the client
                "languageIcon": activity_fields.large_image.unwrap_or_default(),
                "ideIcon": "https://raw.githubusercontent.com/zed-industries/zed/main/assets/icons/zed.ico",
                "workingDirectory": activity_fields.state.unwrap_or_else(|| "Unknown".to_string()),
                "gitUrl": git_remote_url
            }
        });

        let message = Message::Text(presence_data.to_string());
        stream.send(message).await.map_err(|e| {
            error!("Failed to send presence data: {}", e);
            // Mark as disconnected so next attempt will reconnect
            self.connected.store(false, Ordering::SeqCst);
            crate::error::PresenceError::WebSocket(format!("Failed to send presence data: {e}"))
        })?;

        debug!("Presence data sent successfully");
        Ok(())
    }

    /// Sends presence data with automatic reconnection on failure.
    /// If not connected, attempts to reconnect first.
    /// If sending fails, marks connection as disconnected for future reconnection.
    pub async fn send_presence_data_with_reconnect(
        &mut self,
        activity_fields: ActivityFields,
        doc: Option<&Document>,
        git_remote_url: Option<String>,
    ) -> Result<()> {
        // If not connected, try to reconnect first
        if !self.is_connected() {
            warn!("WebSocket not connected, attempting reconnection...");
            if let Err(e) = self.reconnect().await {
                debug!("Reconnection failed: {}", e);
                return Err(e);
            }
        }

        // Try to send presence data
        match self.send_presence_data(activity_fields, doc, git_remote_url).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // Connection may have dropped, mark as disconnected
                warn!("Presence data send failed, marking as disconnected: {}", e);
                self.connected.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_new_defaults() {
        let ws = WebSocketClient::new("ws://localhost:3000/ws".to_string());
        assert!(!ws.is_connected());
        assert!(ws.stream.is_none());
    }

    #[test]
    fn test_is_connected_default_false() {
        let ws = WebSocketClient::new("ws://localhost:3000/ws".to_string());
        assert!(!ws.is_connected());
    }

    #[test]
    fn test_connected_state_can_be_changed() {
        let ws = WebSocketClient::new("ws://localhost:3000/ws".to_string());
        assert!(!ws.is_connected());

        // Manually set connected to true
        ws.connected.store(true, Ordering::SeqCst);
        assert!(ws.is_connected());

        // Set back to false
        ws.connected.store(false, Ordering::SeqCst);
        assert!(!ws.is_connected());
    }

    #[test]
    fn test_retry_constants() {
        // Verify retry constants are reasonable
        assert_eq!(MAX_RETRIES, 5);
        assert_eq!(INITIAL_DELAY_MS, 500);
        assert_eq!(MAX_DELAY_MS, 10_000);
    }
}
