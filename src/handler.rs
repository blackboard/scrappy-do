use crate::callback::Indeterminate;
use reqwest::{Client, Response};
use slog::Logger;
use std::fmt::{self, Debug, Display};
use tokio::sync::mpsc::Receiver;

/// Converts HTTP responses into items or HTTP requests.
///
/// It is the job of the caller to define handlers. There are 2 primary methods of creating one. A
/// conceptually stateless pure function method and a stateful struct implementation.
///
/// # Pure function example
/// Pure function handlers should be your initial implementation and satisfy most needs. All you
/// need to do is define the function and annotate it with `handle`:
///
/// ```
/// // Needed for the yield keyword to work
/// #![feature(generators)]
///
/// use reqwest::{Client, Response};
/// use scrappy_do::handle;
/// use slog::Logger;
///
/// // This is what we are trying to create from the web pages
/// #[derive(Debug)]
/// struct SomeItem;
///
/// // This tracks metadata we find useful during scraping.
/// #[derive(Debug)]
/// struct SomeContext;
///
/// #[handle(item = SomeItem)]
/// fn handler_foo(
///     client: Client,
///     response: Response,
///     context: SomeContext,
///     logger: Logger,
/// ) {
///
///     // Process the response....
///
///     // Return some scraped item
///     yield SomeItem;
/// }
/// ```
///
/// in order to get an actual `Handler` implementation you then just need to `wrap` the function
/// using the provided macro:
/// ```ignore
/// wrap!(handler_foo)
/// ```
///
/// # Struct example
///
/// If your needs are more complex you may need to implement the trait yourself. Luckily the
/// process isn't much more complicated than the pure function method:
///
/// ```
/// // Needed for the yield keyword to work
/// #![feature(generators)]
///
/// use reqwest::{Client, Response};
/// use scrappy_do::{Handler, handle};
/// use slog::Logger;
/// use std::fmt;
///
/// // This is what we are trying to create from the web pages
/// #[derive(Debug)]
/// struct SomeItem;
///
/// // This tracks metadata we find useful during scraping.
/// #[derive(Debug)]
/// struct SomeContext;
///
/// #[derive(Debug)]
/// struct SomeHandler;
///
/// // Handlers need to implement Display
/// impl fmt::Display for SomeHandler {
///     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
///         write!(f, "SomeHandler")
///     }
/// }
///
/// impl Handler<SomeItem, SomeContext> for SomeHandler
/// {
///     #[handle(item = SomeItem)]
///     fn handle(self: Box<Self>,
///               client: Client,
///               response: Response,
///               context: SomeContext,
///               logger: Logger) {
///
///         // Process the response....
///
///         // Return some scraped item
///         yield SomeItem;
///     }
/// }
/// ```
pub trait Handler<I: Debug, C>: Send + Sync + Debug + Display {
    fn handle(
        self: Box<Self>,
        client: Client,
        response: Response,
        context: C,
        logger: Logger,
    ) -> Receiver<Indeterminate<I, C>>;
}

#[doc(hidden)]
pub struct HandlerImpl<F> {
    function: F,
    function_name: &'static str,
}

impl<F> HandlerImpl<F> {
    pub fn new<I: Debug, C>(function: F, function_name: &'static str) -> Self
    where
        F: FnOnce(Client, Response, C, Logger) -> Receiver<Indeterminate<I, C>>
            + Send
            + Sync
            + Copy,
    {
        Self {
            function,
            function_name,
        }
    }
}

impl<F> Debug for HandlerImpl<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerImpl")
            .field("function", &self.function_name)
            .finish()
    }
}

impl<F> Display for HandlerImpl<F> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.function_name)
    }
}

impl<I: Debug, C, F> Handler<I, C> for HandlerImpl<F>
where
    F: FnOnce(Client, Response, C, Logger) -> Receiver<Indeterminate<I, C>> + Send + Sync + Copy,
{
    fn handle(
        self: Box<Self>,
        client: Client,
        response: Response,
        context: C,
        logger: Logger,
    ) -> Receiver<Indeterminate<I, C>> {
        (self.function)(client, response, context, logger)
    }
}
