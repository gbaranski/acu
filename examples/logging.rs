use tokio::sync::oneshot;

#[derive(Debug)]
enum Message {
    Increment,
    Get { respond_to: oneshot::Sender<usize> },
}

struct MyActor {
    receiver: acu::Receiver<Message>,
    counter: usize,
}

impl MyActor {
    async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            match message {
                Message::Increment => self.counter += 1,
                Message::Get { respond_to } => respond_to.send(self.counter).unwrap(),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MyActorHandle {
    sender: acu::Sender<Message>,
}

impl MyActorHandle {
    pub fn new() -> Self {
        let (sender, receiver) = acu::channel(8, "MyActor");
        let mut actor = MyActor {
            receiver,
            counter: 0,
        };
        tokio::spawn(async move { actor.run().await });
        Self { sender }
    }

    pub async fn increment(&self) {
        self.sender.notify(|| Message::Increment).await
    }

    pub async fn get(&self) -> usize {
        self.sender
            .call(|respond_to| Message::Get { respond_to })
            .await
    }
}

#[tokio::main]
async fn main() {
    simple_log::console("debug").unwrap();
    let handle = MyActorHandle::new();
    println!("initial counter: {}", handle.get().await);
    for _ in 0..100 {
        handle.increment().await;
    }
    println!("counter after 100 increments: {}", handle.get().await);
}
