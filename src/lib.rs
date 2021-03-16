#![feature(result_flattening)]

//! A crate to asynchronously crawl web pages. This crate was largely inspired by the python module
//! `scrapy`, but updated to take advantage of Rust's safety, performance, and expressiveness.
//!
//! # About
//! This crate is built around the idea of transforming HTTP responses into scraped `Items`
//! throught dynamically linked handlers. The caller defines the handlers and the crate takes care
//! of the scheduling.
//!
//! # Basic Example
//!
//!The following example shows the most basic handler chain possible:
//!
//! ```no_run
//! // Needed for the yield keyword to work
//! #![feature(generators)]
//!
//! use futures::stream::StreamExt; // Provides friendly methods for streams
//! use reqwest::{Client, Response};
//! use scrappy_do::{handle, wrap};
//! use slog::Logger;
//! use url::Url;
//!
//! // This is what we are trying to create from the web pages
//! #[derive(Debug)]
//! struct SomeItem;
//!
//! // This tracks metadata we find useful during scraping.
//! #[derive(Debug)]
//! struct SomeContext;
//!
//! #[handle(item = SomeItem)]
//! fn handler_foo(
//!     client: Client,
//!     response: Response,
//!     context: SomeContext,
//!     logger: Logger,
//! ) {
//!
//!     // Process the response....
//!
//!     // Return some scraped item
//!     yield SomeItem;
//! }
//!
//! // Sets up multithreaded runtime
//! #[tokio::main]
//! async fn main() {
//!     // Build the HTTP client
//!     let client = reqwest::Client::builder()
//!         .build()
//!         .unwrap();
//!
//!     // Build the spider
//!     let spider = scrappy_do::Spider::new(client.clone(), None);
//!
//!     let items = spider
//!         // A web requires an initial address, handler, and context in order to be created. All
//!         // other configuration is optional.
//!         .web()
//!         .start(
//!             client
//!                 .get(Url::parse("http://somedomain.toscrape.com").unwrap())
//!                 .build()
//!                 .unwrap(),
//!         )
//!         .handler(wrap!(handler_foo))
//!         .context(SomeContext)
//!         .build()
//!         .crawl()
//!         .await;
//!
//!     // The stream must be pinned to the stack to iterate over it
//!     tokio::pin!(items);
//!
//!     // Process the items scraped.
//!     while let Some(item) = items.next().await {
//!         println!("{:?}", item);
//!     }
//! }
//! ```
//!
//! # Chaining Example
//!
//! This example shows a slightly more complicated chain where a handler invokes another handler.
//! The important thing to note is that the caller doesn't change the inital chain invocation at all.
//!
//! ```no_run
//! // Needed for the yield keyword to work
//! #![feature(generators)]
//!
//! use futures::stream::StreamExt; // Provides friendly methods for streams
//! use reqwest::{Client, Response};
//! use scrappy_do::{handle, wrap};
//! use slog::Logger;
//! use url::Url;
//!
//! // This is what we are trying to create from the web pages
//! #[derive(Debug)]
//! struct SomeItem;
//!
//! // This tracks metadata we find useful during scraping.
//! #[derive(Debug)]
//! struct SomeContext;
//!
//!// The inital handler
//! #[handle(item = SomeItem)]
//! fn handler_foo(
//!     client: Client,
//!     response: Response,
//!     context: SomeContext,
//!     logger: Logger,
//! ) {
//!
//!     // Process the response....
//!
//!     let callback = scrappy_do::Callback::new(
//!                wrap!(handler_bar),
//!                client
//!                    .get(response.url().clone())
//!                    .build()
//!                    .unwrap(),
//!                context,
//!            );
//!
//!     // Chain the next callback
//!     yield callback;
//!
//!     // Return some scraped item
//!     yield SomeItem;
//! }
//!
//! // Called by handler_foo
//! #[handle(item = SomeItem)]
//! fn handler_bar(
//!     client: Client,
//!     response: Response,
//!     context: SomeContext,
//!     logger: Logger,
//! ) {
//!
//!     // Process the response....
//!
//!     // Return some scraped item
//!     yield SomeItem;
//! }
//!
//! // Sets up multithreaded runtime
//! #[tokio::main]
//! async fn main() {
//!     // Build the HTTP client
//!     let client = reqwest::Client::builder()
//!         .build()
//!         .unwrap();
//!
//!     // Build the spider
//!     let spider = scrappy_do::Spider::new(client.clone(), None);
//!
//!     let items = spider
//!         // A web requires an initial address, handler, and context in order to be created. All
//!         // other configuration is optional.
//!         .web()
//!         .start(
//!             client
//!                 .get(Url::parse("http://somedomain.toscrape.com").unwrap())
//!                 .build()
//!                 .unwrap(),
//!         )
//!         .handler(wrap!(handler_foo))
//!         .context(SomeContext)
//!         .build()
//!         .crawl()
//!         .await;
//!
//!     // The stream must be pinned to the stack to iterate over it
//!     tokio::pin!(items);
//!
//!     // Process the items scraped.
//!     while let Some(item) = items.next().await {
//!         println!("{:?}", item);
//!     }
//! }
//! ```
//!
//! Ultimately the chain can be as complicated or as simple as the caller wants. It can even be
//! never ending if need be and the `Web` will just keep happily chugging along processing
//! handlers.
//!
pub use scrappy_do_codegen::*;

mod callback;
mod handler;
mod spider;
pub mod util;
pub use callback::{Callback, Indeterminate};
pub use handler::{Handler, HandlerImpl};
pub use spider::{Spider, Web, WebBuilder};

#[doc(hidden)]
pub use tokio::{
    spawn,
    sync::mpsc::{channel, Receiver},
};
