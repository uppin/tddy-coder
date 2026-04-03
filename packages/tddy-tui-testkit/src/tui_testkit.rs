//! Playwright-style wrapper around a [`VirtualTuiSession`]: VT100 screen + input helpers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tokio::sync::Mutex;
use vt100::Parser;

use tddy_service::VirtualTuiSession;

use crate::input_encoding::{encode_resize, event_to_bytes};

/// Harness for driving VirtualTui in tests: keyboard, mouse, resize, and VT100 screen queries.
pub struct TuiTestkit {
    input_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    output_rx: Mutex<tokio::sync::mpsc::Receiver<Vec<u8>>>,
    shutdown: Arc<AtomicBool>,
    accumulated: Mutex<Vec<u8>>,
    dimensions: Mutex<(u16, u16)>,
}

impl TuiTestkit {
    /// Wraps an existing [`VirtualTuiSession`] and initial terminal size (cols × rows).
    pub fn new(session: VirtualTuiSession, cols: u16, rows: u16) -> Self {
        let VirtualTuiSession {
            input_tx,
            output_rx,
            shutdown,
        } = session;
        Self {
            input_tx,
            output_rx: Mutex::new(output_rx),
            shutdown,
            accumulated: Mutex::new(Vec::new()),
            dimensions: Mutex::new((cols, rows)),
        }
    }

    async fn drain_output(&self) {
        let mut chunks = Vec::new();
        {
            let mut rx = self.output_rx.lock().await;
            while let Ok(chunk) = rx.try_recv() {
                chunks.push(chunk);
            }
        }
        let mut acc = self.accumulated.lock().await;
        for c in chunks {
            acc.extend(c);
        }
    }

    fn parse_screen(acc: &[u8], cols: u16, rows: u16) -> String {
        let mut parser = Parser::new(rows, cols, 0);
        parser.process(acc);
        parser.screen().contents()
    }

    async fn send_bytes(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.input_tx
            .send(bytes)
            .await
            .map_err(|_| anyhow::anyhow!("VirtualTui input channel closed"))
    }

    /// Full screen text from the VT100 model (rows separated by newlines).
    pub async fn screen_contents(&self) -> String {
        self.drain_output().await;
        let (cols, rows, bytes) = {
            let dims = self.dimensions.lock().await;
            let acc = self.accumulated.lock().await;
            (dims.0, dims.1, acc.clone())
        };
        Self::parse_screen(&bytes, cols, rows)
    }

    /// One logical row of the visible screen (`row` 0 = top).
    pub async fn screen_line(&self, row: usize) -> String {
        let contents = self.screen_contents().await;
        contents.lines().nth(row).unwrap_or("").to_string()
    }

    /// Whether the visible screen contains `text` as a substring.
    pub async fn screen_contains(&self, text: &str) -> bool {
        self.screen_contents().await.contains(text)
    }

    /// Press a single key (press kind, no modifiers).
    pub async fn press_key(&self, key: KeyCode) -> anyhow::Result<()> {
        self.press_key_modified(key, KeyModifiers::empty()).await
    }

    /// Press a key with modifiers.
    pub async fn press_key_modified(
        &self,
        key: KeyCode,
        modifiers: KeyModifiers,
    ) -> anyhow::Result<()> {
        let ev = KeyEvent::new(key, modifiers);
        let event = Event::Key(ev);
        let bytes =
            event_to_bytes(&event).with_context(|| format!("no bytes for key {:?}", key))?;
        self.send_bytes(bytes).await
    }

    /// Type UTF-8 text as individual key presses.
    pub async fn type_text(&self, text: &str) -> anyhow::Result<()> {
        for ch in text.chars() {
            let ev = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty());
            let bytes = event_to_bytes(&Event::Key(ev))
                .with_context(|| format!("no bytes for character {:?}", ch))?;
            self.send_bytes(bytes).await?;
        }
        Ok(())
    }

    pub async fn press_enter(&self) -> anyhow::Result<()> {
        self.press_key(KeyCode::Enter).await
    }

    /// Left mouse button down at cell (`col`, `row`) using crossterm 0-based coordinates.
    pub async fn click(&self, col: u16, row: u16) -> anyhow::Result<()> {
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        };
        let bytes = event_to_bytes(&Event::Mouse(ev)).context("mouse down encoding")?;
        self.send_bytes(bytes).await
    }

    /// Mouse wheel up at cell (`col`, `row`).
    pub async fn scroll_up(&self, col: u16, row: u16) -> anyhow::Result<()> {
        let ev = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        };
        let bytes = event_to_bytes(&Event::Mouse(ev)).context("scroll up encoding")?;
        self.send_bytes(bytes).await
    }

    /// Mouse wheel down at cell (`col`, `row`).
    pub async fn scroll_down(&self, col: u16, row: u16) -> anyhow::Result<()> {
        let ev = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        };
        let bytes = event_to_bytes(&Event::Mouse(ev)).context("scroll down encoding")?;
        self.send_bytes(bytes).await
    }

    /// Resize the virtual terminal and update the VT100 model dimensions.
    pub async fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        {
            let mut d = self.dimensions.lock().await;
            *d = (cols, rows);
        }
        self.send_bytes(encode_resize(cols, rows)).await
    }

    /// Poll until `text` appears on the parsed screen or `timeout` elapses.
    pub async fn wait_for_text(&self, text: &str, timeout: Duration) -> anyhow::Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            self.drain_output().await;
            let found = {
                let dims = self.dimensions.lock().await;
                let acc = self.accumulated.lock().await;
                Self::parse_screen(&acc, dims.0, dims.1).contains(text)
            };
            if found {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::bail!("timeout waiting for text: {text:?}")
    }

    /// Wait until output stops growing for a short quiet period, or until `timeout`.
    pub async fn wait_for_render(&self, timeout: Duration) -> anyhow::Result<()> {
        let quiet = Duration::from_millis(80);
        let deadline = Instant::now() + timeout;
        let mut last_len = 0usize;
        let mut last_change = Instant::now();
        loop {
            self.drain_output().await;
            let len = self.accumulated.lock().await.len();
            if len != last_len {
                last_len = len;
                last_change = Instant::now();
            }
            if len > 0 && last_change.elapsed() >= quiet {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    }

    /// Signal VirtualTui to shut down.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Input sender for advanced scenarios (same channel VirtualTui reads).
    pub fn input_sender(&self) -> &tokio::sync::mpsc::Sender<Vec<u8>> {
        &self.input_tx
    }
}
