# Rust Advanced Features — Async & Shared State Refresher

Aimed at the Prosopon client: async, shared state across threads, WebRTC callbacks.
Not a textbook dump — the things you'll actually hit.

---

## 1. The two marker traits that gate everything: `Send` and `Sync`

*Auto traits* — the compiler derives them for you. They decide what can cross a thread boundary.

- **`Send`** — the type can be *moved* to another thread (ownership transfer).
- **`Sync`** — the type can be *shared* across threads by reference (`&T` is `Send`).

Rule of thumb: `T: Sync` ⟺ `&T: Send`.

Why you care: `spawn` requires everything moved in to be `Send`. Sharing via `Arc` requires the inner type to be `Send + Sync`. If the compiler says "`X` cannot be shared between threads safely," this is the missing trait.

## 2. `Arc<T>` — shared ownership

`Rc<T>` is single-threaded reference counting. `Arc<T>` is the atomic version — safe across threads.

```rust
use std::sync::Arc;

let data = Arc::new(Config { /* ... */ });
let clone = Arc::clone(&data);  // cheap: just bumps a counter
```

`Arc::clone` is cheap (atomic increment), so clone freely. Data frees when the last `Arc` drops.

## 3. `Mutex<T>` and `RwLock<T>` — interior mutability, synchronized

`Arc<T>` gives shared *read* access. To mutate shared state, wrap it:

```rust
use std::sync::{Arc, Mutex};

let counter = Arc::new(Mutex::new(0));

// in a thread:
let mut guard = counter.lock().unwrap();
*guard += 1;
// guard drops here, releasing the lock
```

- **`Mutex`** — exclusive access. One writer at a time.
- **`RwLock`** — many readers *or* one writer. Use when reads vastly outnumber writes.

The classic pattern is `Arc<Mutex<T>>` — shared, mutable, thread-safe state.

## 4. `Arc<Mutex<T>>` vs `tokio::sync::Mutex<T>` — the one that trips everyone

Two different mutexes:

- **`std::sync::Mutex`** — blocks the *thread* while waiting. Use when the critical section is short and non-blocking (no `.await` inside the lock).
- **`tokio::sync::Mutex`** — yields the *task* while waiting (async). Use when you need to hold the lock across an `.await`.

**The rule:** never hold a `std::sync::Mutex` guard across an `.await`. It blocks the whole thread, and if that thread is a tokio worker, you've frozen the runtime.

```rust
// WRONG — blocks the worker thread across the await
let guard = std_mutex.lock().unwrap();
something().await;  // deadlock risk

// RIGHT — async mutex for await-holding
let guard = tokio_mutex.lock().await;
something().await;
```

Default to `std::sync::Mutex` for short critical sections (faster). Reach for `tokio::sync::Mutex` only when you must `.await` while holding it.

## 5. `async`/`await` and `Future`

An `async fn` or `async {}` block returns a `Future` — a value that produces a result *later*. A future does nothing until polled (driven by the runtime).

```rust
async fn fetch() -> String { /* ... */ }

let fut = fetch();        // nothing runs yet — just a future
let result = fut.await;   // now it runs to completion
```

Key facts:
- Futures are **lazy**. No `.await`, no execution.
- `.await` yields control back to the runtime while waiting, so other tasks can run.
- A future is a state machine. It captures everything it needs across `.await` points.

## 6. `Pin<Box<dyn Future>>` — why async sometimes needs pinning

A future can hold references to *its own* fields (self-referential). If the future moves in memory, those references dangle. `Pin` promises "this won't move."

You mostly hit this when:
- Boxing a future: `Box::pin(async { ... })`
- Storing futures in a struct
- Using `async fn` in trait objects (`Box<dyn Future<Output = T> + Send>`)

```rust
use std::pin::Pin;
use std::future::Future;

fn boxed() -> Pin<Box<dyn Future<Output = i32> + Send>> {
    Box::pin(async { 42 })
}
```

For the client, you'll see `Pin<Box<...>>` in WebRTC callback signatures. Don't fight it — it means "a heap-allocated future that won't move."

## 7. `tokio::sync` channels — the async communication toolkit

When you don't want shared mutable state at all (often cleaner), use channels:

- **`mpsc`** — many producers, one consumer. The workhorse.
- **`oneshot`** — one value, one time. Perfect for request/response.
- **`watch`** — broadcast the *latest* value (subscribers see only the newest). Good for config/state.
- **`broadcast`** — every message to every subscriber. Good for events.
- **`Notify`** — a simple "wake up" signal, no data.

```rust
use tokio::sync::mpsc;

let (tx, mut rx) = mpsc::channel(32);
tx.send("hello".to_string()).await.unwrap();
let msg = rx.recv().await.unwrap();
```

For the client: `mpsc` for signaling/data-channel messages, `oneshot` for "send SDP, get answer back," `watch` for connection state.

## 8. `OnceLock` and `LazyLock` — one-time initialization

For global singletons (config, a shared client handle):

```rust
use std::sync::OnceLock;

static CONFIG: OnceLock<Config> = OnceLock::new();

fn config() -> &'static Config {
    CONFIG.get_or_init(|| Config::load())
}
```

`LazyLock` is the same but initializes on first *use* automatically. Use these instead of `lazy_static!` (the old crate) — they're now in `std`.

## 9. `dyn Trait` vs `impl Trait` — dynamic vs static dispatch

- **`impl Trait`** — "some concrete type that implements this." Static dispatch, monomorphized, fast. Used in return position and args.
- **`dyn Trait`** — "any type implementing this, behind a pointer." Dynamic dispatch, a vtable lookup. Needs `Box<dyn Trait>` or `&dyn Trait`.

```rust
fn make() -> impl Future<Output = i32> { async { 1 } }   // static
fn boxed() -> Box<dyn Future<Output = i32> + Send> { Box::pin(async { 1 }) }  // dynamic
```

For trait objects that cross threads, add `+ Send + Sync` as needed.

## 10. Lifetimes and `'static` — why async demands it

`tokio::spawn` requires the future to be `'static` — it can't borrow from the local stack, because the task might outlive the function that spawned it.

```rust
// WRONG — borrows local data, can't be 'static
let local = String::from("hi");
tokio::spawn(async { println!("{}", local) });  // fails

// RIGHT — move ownership in
let local = String::from("hi");
tokio::spawn(async move { println!("{}", local) });
```

`async move` takes ownership of captured variables. When you see `'static` errors in async, the fix is almost always: `Arc` + `move`, or restructure to own the data.

---

## The one-sentence mental model

> **`Arc` to share, `Mutex`/`RwLock` to mutate, `Send + Sync` to cross threads, `async move` to own, channels to communicate, `Pin<Box<dyn Future>>` when the compiler insists.**

For the Prosopon client specifically, expect to see: `Arc<Mutex<ConnectionState>>` for the WebRTC state, `mpsc` for signaling messages, `oneshot` for the SDP handshake, and `Pin<Box<dyn Future + Send>>` in the callback signatures.
