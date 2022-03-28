# acu

Utility crate for building asynchronous actors.

Before using this crate, I'd recommend to get to know of the actor pattern in Rust, [Alice Ryhl](https://ryhl.io) created a very useful [blog post](https://ryhl.io/blog/actors-with-tokio/).

## Getting started

### Add crate to dependencies

Using [cargo-edit](https://github.com/killercup/cargo-edit)
```
cargo add acu
```
or manually...

### Build your first Actor

```rust
use tokio::sync::oneshot;

#[derive(Debug)]
enum Message {
    Increment,
    Get { respond_to: oneshot::Sender<usize> },
}

impl acu::Message for Message {}

struct MyActor {
    receiver: acu::Receiver<Message, &'static str>,
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
    sender: acu::Sender<Message, &'static str>,
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
        self.sender.notify_with(|| Message::Increment).await
    }

    pub async fn get(&self) -> usize {
        self.sender
            .call_with(|respond_to| Message::Get { respond_to })
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

or if you would like to make use of logging functionality, you need to initialize `log`, for example by using [simple-log](https://lib.rs/crates/simple-log) crate:
```rust
// at the top of the main function
simple_log::quick!("debug");
```

Then each call/notify on the actor will get logged.


### Master/slave pattern

You need to have `master-slave` feature enabled for the crate.

The decision you need to make, is whether the Actor Message implements `Clone` trait, if yes you can use `BroadcasterMasterHandle` which allows you to use directly actor methods; if no, you're stuck with `MasterHandle` on which you can't use actor methods.

#### Using `BroadcasterMasterHandle`(Message: Clone)

```rust
use acu::BroadcasterMasterHandle;
use acu::MasterExt;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Name {
    Master,
    MyActorA,
    MyActorB,
}

impl acu::MasterName for Name {
    fn master_name() -> Self {
        Self::Master
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        match self {
            Name::Master => "master",
            Name::MyActorA => "my-actor-a",
            Name::MyActorB => "my-actor-b",
        }
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &str = self.as_ref();
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
enum Message {
    Increment,
    Fetch {
        respond_to: broadcast::Sender<usize>,
    },
}

impl acu::Message for Message {}

struct MyActor {
    receiver: acu::Receiver<Message, Name>,
    counter: usize,
}

impl MyActor {
    async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            match message {
                Message::Increment => self.counter += 1,
                Message::Fetch { respond_to } => {
                    respond_to.send(self.counter).unwrap();
                }
            }
        }
    }
}

fn my_actor(name: Name) -> MyActorHandle {
    let (sender, receiver) = acu::channel(name);
    let mut actor = MyActor {
        receiver,
        counter: 0,
    };
    tokio::spawn(async move { actor.run().await });
    MyActorHandle { sender }
}

type MyActorHandle = acu::Handle<Message, Name>;

use async_trait::async_trait;

#[async_trait]
trait MyActorExt {
    async fn increment(&self);
    async fn fetch(&self) -> Vec<usize>;
}

#[async_trait]
impl MyActorExt for MyActorHandle {
    async fn increment(&self) {
        self.sender.notify_with(|| Message::Increment).await
    }

    async fn fetch(&self) -> Vec<usize> {
        self.sender
            .call_many_with(|respond_to| Message::Fetch { respond_to }, 8)
            .await
    }
}

#[tokio::main]
async fn main() {
    let handle_a = my_actor(Name::MyActorA);
    let handle_b = my_actor(Name::MyActorB);
    let master = {
        let master = BroadcasterMasterHandle::new();
        master.push(handle_a).await;
        master.push(handle_b).await;
        master
    };
    let get_values = || async {
        let results = master.fetch().await;
        assert_eq!(results.len(), 2);
        (results[0], results[1])
    };
    let print_values = || async {
        let values = get_values().await;
        println!("counter of MyActorA = {}", values.0);
        println!("counter of MyActorB = {}", values.1);
        println!();
    };
    for _ in 0..100 {
        master.increment().await;
        print_values().await;
    }
    print_values().await;
    {
        let actor_a = master.find(Name::MyActorA).await.unwrap();
        for _ in 0..10 {
            actor_a.increment().await;
        }
    }
    print_values().await;
}
```

#### Using `MasterHandle`(Message: ?Clone)

```rust
use acu::MasterHandle;
use acu::MasterExt;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Name {
    Master,
    MyActorA,
    MyActorB,
}

impl acu::MasterName for Name {
    fn master_name() -> Self {
        Self::Master
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        match self {
            Name::Master => "master",
            Name::MyActorA => "my-actor-a",
            Name::MyActorB => "my-actor-b",
        }
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &str = self.as_ref();
        f.write_str(s)
    }
}

#[derive(Debug)]
enum Message {
    Increment,
    Fetch {
        respond_to: oneshot::Sender<usize>,
    },
}

impl acu::Message for Message {}

struct MyActor {
    receiver: acu::Receiver<Message, Name>,
    counter: usize,
}

impl MyActor {
    async fn run(&mut self) {
        while let Some(message) = self.receiver.recv().await {
            match message {
                Message::Increment => self.counter += 1,
                Message::Fetch { respond_to } => {
                    respond_to.send(self.counter).unwrap();
                }
            }
        }
    }
}

fn my_actor(name: Name) -> MyActorHandle {
    let (sender, receiver) = acu::channel(name);
    let mut actor = MyActor {
        receiver,
        counter: 0,
    };
    tokio::spawn(async move { actor.run().await });
    MyActorHandle { sender }
}

type MyActorHandle = acu::Handle<Message, Name>;

use async_trait::async_trait;

#[async_trait]
trait MyActorExt {
    async fn increment(&self);
    async fn fetch(&self) -> usize;
}

#[async_trait]
impl MyActorExt for MyActorHandle {
    async fn increment(&self) {
        self.sender.notify_with(|| Message::Increment).await
    }

    async fn fetch(&self) -> usize {
        self.sender
            .call_with(|respond_to| Message::Fetch { respond_to })
            .await
    }
}

#[tokio::main]
async fn main() {
    let handle_a = my_actor(Name::MyActorA);
    let handle_b = my_actor(Name::MyActorB);
    let master = {
        let master = MasterHandle::new();
        master.push(handle_a).await;
        master.push(handle_b).await;
        master
    };
    let get_handles = || async {
        let handle_a = master.find(Name::MyActorA).await.unwrap();
        let handle_b = master.find(Name::MyActorA).await.unwrap();
        (handle_a, handle_b)
    };
    let get_values = || async {
        let (handle_a, handle_b) = get_handles().await;
        (handle_a.fetch().await, handle_b.fetch().await)
    };
    let print_values = || async {
        let values = get_values().await;
        println!("counter of MyActorA = {}", values.0);
        println!("counter of MyActorB = {}", values.1);
        println!();
    };
    for _ in 0..100 {
        let (handle_a, handle_b) = get_handles().await;
        handle_a.increment().await;
        handle_b.increment().await;
        print_values().await;
    }
    print_values().await;
    {
        let actor_a = master.find(Name::MyActorA).await.unwrap();
        for _ in 0..10 {
            actor_a.increment().await;
        }
    }
    print_values().await;
}
```

All examples can be found in `examples/` directory.


## Motivation

I wanted to use some structs and functions in few of my projects, including [Houseflow](https://github.com/gbaranski/houseflow). And I thought this might be useful for other projects as well.