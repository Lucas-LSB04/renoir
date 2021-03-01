use std::collections::VecDeque;

use async_trait::async_trait;

use crate::network::{NetworkMessage, NetworkReceiver};
use crate::operator::{Operator, StreamElement};
use crate::scheduler::ExecutionMetadata;

pub struct StartBlock<Out>
where
    Out: Clone + Send + 'static,
{
    metadata: Option<ExecutionMetadata>,
    receiver: Option<NetworkReceiver<NetworkMessage<Out>>>,
    buffer: VecDeque<StreamElement<Out>>,
    missing_ends: usize,
}

impl<Out> StartBlock<Out>
where
    Out: Clone + Send + 'static,
{
    pub fn new() -> Self {
        StartBlock {
            metadata: None,
            receiver: None,
            buffer: Default::default(),
            missing_ends: 0,
        }
    }
}

#[async_trait]
impl<Out> Operator<Out> for StartBlock<Out>
where
    Out: Clone + Send + 'static,
{
    async fn setup(&mut self, metadata: ExecutionMetadata) {
        let mut network = metadata.network.lock().await;
        self.receiver = Some(network.get_receiver(metadata.coord));
        drop(network);
        self.missing_ends = metadata.num_prev;
        info!(
            "StartBlock {} initialized, {} previous blocks, receiver is: {:?}",
            metadata.coord, metadata.num_prev, self.receiver
        );
        self.metadata = Some(metadata);
    }

    async fn next(&mut self) -> StreamElement<Out> {
        let metadata = self.metadata.as_ref().unwrap();
        // all the previous blocks sent and end: we're done
        if self.missing_ends == 0 {
            info!("StartBlock for {} has ended", metadata.coord);
            return StreamElement::End;
        }
        let receiver = self.receiver.as_ref().unwrap();
        if self.buffer.is_empty() {
            // receive from any previous block
            let buf = receiver.recv().await.unwrap();
            self.buffer.append(&mut buf.into());
        }
        let message = self
            .buffer
            .pop_front()
            .expect("Previous block sent an empty message");
        if matches!(message, StreamElement::End) {
            self.missing_ends -= 1;
            debug!(
                "{} received an end, {} more to come",
                metadata.coord, self.missing_ends
            );
            return self.next().await;
        }
        debug!("Block {} received a message", metadata.coord);
        message
    }

    fn to_string(&self) -> String {
        format!("[{}]", std::any::type_name::<Out>())
    }
}

impl<Out> Clone for StartBlock<Out>
where
    Out: Clone + Send + 'static,
{
    fn clone(&self) -> Self {
        if self.metadata.is_some() {
            panic!("Cannot clone once initialized");
        }
        Self {
            metadata: None,
            receiver: None,
            buffer: Default::default(),
            missing_ends: 0,
        }
    }
}
