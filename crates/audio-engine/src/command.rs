use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("real-time command queue is full")]
pub struct QueueFull;

pub struct CommandSender<T> {
    queue: Arc<ArrayQueue<T>>,
}

impl<T> Clone for CommandSender<T> {
    fn clone(&self) -> Self {
        Self {
            queue: Arc::clone(&self.queue),
        }
    }
}

impl<T> CommandSender<T> {
    pub fn try_send(&self, command: T) -> Result<(), QueueFull> {
        self.queue.push(command).map_err(|_| QueueFull)
    }

    pub fn remaining_capacity(&self) -> usize {
        self.queue.capacity() - self.queue.len()
    }
}

pub struct CommandReceiver<T> {
    queue: Arc<ArrayQueue<T>>,
}

impl<T> CommandReceiver<T> {
    pub fn try_receive(&self) -> Option<T> {
        self.queue.pop()
    }
}

pub fn bounded_command_queue<T>(capacity: usize) -> (CommandSender<T>, CommandReceiver<T>) {
    assert!(capacity > 0, "command queue capacity must be non-zero");
    let queue = Arc::new(ArrayQueue::new(capacity));
    (
        CommandSender {
            queue: Arc::clone(&queue),
        },
        CommandReceiver { queue },
    )
}

#[cfg(test)]
mod tests {
    use super::{QueueFull, bounded_command_queue};

    #[test]
    fn queue_is_bounded_and_fifo() {
        let (sender, receiver) = bounded_command_queue(2);
        sender.try_send(10).expect("first command should fit");
        sender.try_send(20).expect("second command should fit");

        assert!(matches!(sender.try_send(30), Err(QueueFull)));
        assert_eq!(receiver.try_receive(), Some(10));
        assert_eq!(receiver.try_receive(), Some(20));
        assert_eq!(receiver.try_receive(), None);
    }
}
