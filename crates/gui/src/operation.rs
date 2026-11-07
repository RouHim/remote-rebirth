//! Running a remote operation off the UI thread.
//!
//! Work never blocks the window: it runs on a worker thread, reports [`Progress`] as it goes, and
//! ends with one [`OpDone`] or one error string. The cancel token is checked between steps.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use iced::Task;
use iced::futures::StreamExt;
use iced::task::{Straw, sipper};
use remote_rebirth_core::Progress;

use crate::message::OpDone;

pub(crate) fn operation_task<F>(work: F) -> Task<crate::message::Message>
where
    F: FnOnce(&mut dyn FnMut(Progress)) -> Result<OpDone, String> + Send + 'static,
{
    Task::sip(
        operation(work),
        crate::message::Message::Progress,
        crate::message::Message::Done,
    )
}

enum OperationEvent {
    Progress(Progress),
    Done(Result<OpDone, String>),
}

fn operation<F>(work: F) -> impl Straw<OpDone, Progress, String>
where
    F: FnOnce(&mut dyn FnMut(Progress)) -> Result<OpDone, String> + Send + 'static,
{
    sipper(async move |mut progress| {
        let (sender, mut receiver) = iced::futures::channel::mpsc::unbounded();
        std::thread::spawn(move || {
            let mut sink = |event: Progress| {
                let _ = sender.unbounded_send(OperationEvent::Progress(event));
            };
            let result = work(&mut sink);
            let _ = sender.unbounded_send(OperationEvent::Done(result));
        });
        while let Some(event) = receiver.next().await {
            match event {
                OperationEvent::Progress(event) => {
                    let _ = progress.send(event).await;
                }
                OperationEvent::Done(result) => return result,
            }
        }
        Err("operation ended without a result".into())
    })
}

pub(crate) fn check_cancel(cancel: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled before the commit.".into())
    } else {
        Ok(())
    }
}
