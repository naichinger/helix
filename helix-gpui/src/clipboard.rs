//! Clipboard calls originate on the editor thread and are serviced on GPUI's
//! foreground thread. The UI never waits on the editor, so requests cannot
//! deadlock with rendering or input dispatch.
use helix_view::clipboard::{ClipboardError, ClipboardType, NativeClipboard};
use std::{sync::mpsc, time::Duration};
use tokio::sync::mpsc::UnboundedSender;

pub enum Request {
    Get(ClipboardType, mpsc::SyncSender<String>),
    Set(ClipboardType, String),
}

#[derive(Debug)]
pub struct Clipboard(pub UnboundedSender<Request>);

impl NativeClipboard for Clipboard {
    fn get(&self, kind: ClipboardType) -> Result<String, ClipboardError> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.0
            .send(Request::Get(kind, tx))
            .map_err(|_| ClipboardError::ReadingNotSupported)?;
        rx.recv_timeout(Duration::from_secs(2))
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::TimedOut, error).into())
    }
    fn set(&self, text: &str, kind: ClipboardType) -> Result<(), ClipboardError> {
        self.0
            .send(Request::Set(kind, text.into()))
            .map_err(|_| ClipboardError::StdinWriteFailed)
    }
}

pub fn connect(mut requests: tokio::sync::mpsc::UnboundedReceiver<Request>, cx: &mut gpui::App) {
    cx.spawn(async move |cx| {
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let mut primary = String::new();
        while let Some(request) = requests.recv().await {
            let _ = cx.update(|cx| match request {
                Request::Get(ClipboardType::Clipboard, reply) => {
                    let _ = reply.send(
                        cx.read_from_clipboard()
                            .and_then(|item| item.text())
                            .unwrap_or_default(),
                    );
                }
                Request::Get(ClipboardType::Selection, reply) => {
                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                    let primary = cx
                        .read_from_primary()
                        .and_then(|item| item.text())
                        .unwrap_or_default();
                    let _ = reply.send(primary.clone());
                }
                Request::Set(ClipboardType::Clipboard, text) => {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text))
                }
                Request::Set(ClipboardType::Selection, text) => {
                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                    cx.write_to_primary(gpui::ClipboardItem::new_string(text));
                    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
                    {
                        primary = text;
                    }
                }
            });
        }
    })
    .detach();
}
