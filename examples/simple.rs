// Needed for the yield keyword to work
#![feature(generators)]

use futures::stream::StreamExt; // Provides friendly methods for streams
use reqwest::{Client, Response};
use scraper::{Html, Selector}; // Used to parse Responses with CSS selectors
use scrappy_do::{
    handle,
    util::{get_unique_element, parse_attr},
    wrap, Callback, Spider,
};
use slog::{info, Logger};

// This is the `Item` eg what we are trying to create from the web pages
#[derive(Debug)]
struct Quote {
    person: String,
    quote: String,
    tags: Vec<String>,
}

#[handle(item = Quote)]
fn parse_quotes(client: Client, response: Response, context: u8, logger: Logger) {
    // We grab the URL first because grabbing the body consumes the response
    let url = response.url().clone();

    // Grab the response body and consume the response
    let body = response.text().await.unwrap();

    // Create a Vector to store the parsed quotes
    let mut quotes = Vec::new();

    // Inner block to drop mutable objects before yield calls. This is needed because yields are
    // async calls and the execution could switch threads between them.
    let next_page = {
        let fragment = Html::parse_document(&body);

        // Generate CSS selectors to find the HTML tags cared about
        let quote_selector = Selector::parse(".quote").unwrap();
        let text_selector = Selector::parse(".text").unwrap();
        let person_selector = Selector::parse("small").unwrap();
        let tag_selector = Selector::parse(".tag").unwrap();

        // Iterate over the found quotes
        for quote in fragment.select(&quote_selector) {
            let text = get_unique_element(&mut quote.select(&text_selector))
                .unwrap()
                .inner_html();
            let person = get_unique_element(&mut quote.select(&person_selector))
                .unwrap()
                .inner_html();
            let tags = quote
                .select(&tag_selector)
                .map(|element| element.inner_html())
                .collect();
            quotes.push(Quote {
                quote: text,
                person,
                tags,
            });
        }

        // Grab the link to the next page
        let next_selector = Selector::parse(".next a").unwrap();
        parse_attr(&mut fragment.select(&next_selector), "href").ok()
    };

    // We only want to scrape the first 2 pages of quotes
    if context < 2 {
        if let Some(link) = next_page {
            info!(logger, "Found next page"; "link" => &link);
            let callback = Callback::new(
                wrap!(parse_quotes),
                client
                    .get(url.join(&link).unwrap().as_str())
                    .build()
                    .unwrap(),
                context + 1,
            );
            yield callback;
        }
    }

    // We yield the results last so that the next callback can start being processed
    for quote in quotes {
        yield quote;
    }
}

// Sets up multithreaded runtime
#[tokio::main]
async fn main() {
    // Build the HTTP client
    let client = reqwest::Client::builder().build().unwrap();

    // Build the spider passing None as the logger because we don't feel like configuring slog. A
    // logger compatible with the `log` crate will be created and passed to the handler's functions.
    let spider = Spider::new(client.clone(), None);

    let items = spider
        // A web requires an initial address, handler, and context in order to be created. All
        // other configuration is optional.
        .web()
        .start(
            client
                .get("http://quotes.toscrape.com/page/1/")
                .build()
                .unwrap(),
        )
        .handler(wrap!(parse_quotes))
        // We will just pass a number as the context because all we care about tracking is the page
        // number
        .context(1)
        .build()
        .crawl()
        .await;

    // The stream must be pinned to the stack to iterate over it
    tokio::pin!(items);

    // Process the items scraped.
    while let Some(item) = items.next().await {
        println!("{:#?}", item);
    }
}
