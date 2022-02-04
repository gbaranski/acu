# acu

Utility crate for building asynchronous actors.

Before using this crate, I'd recommend to get to know of the actor pattern in Rust, [Alice Ryhl](https://ryhl.io) created a very useful [blog post](https://ryhl.io/blog/actors-with-tokio/).

## Getting started

##### Add crate to dependencies

Using [cargo-edit](https://github.com/killercup/cargo-edit)
```
cargo add acu
```
or manually...

##### Build your first Actor

```rust
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
    let handle = MyActorHandle::new();
    println!("initial counter: {}", handle.get().await);
    for _ in 0..100 {
        handle.increment().await;
    }
    println!("counter after 100 increments: {}", handle.get().await);
}
```

or if you would like to make use of logging functionality, add `log` feature to the `acu` dependency, and initialize `log`, for example by using [simple-log](https://lib.rs/crates/simple-log) crate:
```rust
// at the top of the main function
simple_log::quick!("debug");
```

Then each call/notify on the actor will get logged.

## Motivation

I wanted to use some structs and functions in few of my projects, including [Houseflow](https://github.com/gbaranski/houseflow). And I thought this might be useful for other projects as well.