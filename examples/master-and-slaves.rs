use futures::FutureExt;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Name {
    MyActorA,
    MyActorB,
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        match self {
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

impl acu::Name for Name {}

#[derive(Debug)]
enum Message {
    Increment,
    Get { respond_to: oneshot::Sender<usize> },
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
                Message::Get { respond_to } => respond_to.send(self.counter).unwrap(),
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MyActorHandle {
    sender: acu::Sender<Message, Name>,
}

impl acu::Handle<Message, Name> for MyActorHandle {
    fn sender(&self) -> &acu::Sender<Message, Name> {
        &self.sender
    }
}

impl MyActorHandle {
    pub fn new(name: Name) -> Self {
        let (sender, receiver) = acu::channel(8, name);
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
    let handle_a = MyActorHandle::new(Name::MyActorA);
    let handle_b = MyActorHandle::new(Name::MyActorB);
    let master = acu::MasterHandle::new(vec![handle_a.clone(), handle_b.clone()]);
    let get_values = || async {
        let results = master.execute_on_all(|handle| handle.get().boxed()).await;
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
        master
            .execute_on_all(|handle| handle.increment().boxed())
            .await;
    }
    print_values().await;
    {
        let actor_a = master.get(Name::MyActorA).unwrap();
        for _ in 0..10 {
            actor_a.increment().await;
        }
    }
    print_values().await;
}
