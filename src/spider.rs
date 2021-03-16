use crate::callback::{Callback, Indeterminate};
use crate::handler::Handler;
use futures::{
    stream::StreamExt, // for `next`
    Stream,
};
use reqwest::{Client, Request};
use slog::{crit, debug, error, info, o, Drain, Logger};
use std::fmt::Debug;
use std::num::NonZeroUsize;
use thiserror::Error;
use tokio::{
    spawn,
    sync::mpsc::{channel, error::SendError, unbounded_channel, Sender, UnboundedSender},
};

#[derive(Error, Debug)]
pub(crate) enum Error<I, C>
where
    I: Debug,
    C: Debug,
{
    #[error("a task was not able to be added to the task queue: {0:?}")]
    TaskQueue(SendError<PendingCallback<I, C>>),
    #[error("was not able to add the item (given: {0:?}) to the item queue")]
    ItemQueue(SendError<I>),
    #[error("an error occured executing the callback: {0}")]
    Callback(reqwest::Error),
}

/// Creates webs to be used to asynchronously and concurrently crawl a webpage. Internal
/// state is lightweight so it is unecessary to wrap in `Arc` or `Rc`. Multithreading can be
/// achieved by using the multithreaded [tokio](tokio) runtime.
#[derive(Clone)]
pub struct Spider {
    client: Client,
    logger: Logger,
}

impl Spider {
    /// Create a new `Spider` to generate [Webs](Web).
    ///
    /// # Arguments
    ///
    /// * `client` - The client used to make requests.
    /// * `logger` - Used to log messages.
    pub fn new<L: Into<Option<Logger>>>(client: Client, logger: L) -> Self {
        Self {
            client,
            logger: logger
                .into()
                .unwrap_or_else(|| slog::Logger::root(slog_stdlog::StdLog.fuse(), o!())),
        }
    }

    /// Create a new `WebBuilder`.
    pub fn web<H, I, C>(&self) -> WebBuilder<H, C>
    where
        H: Handler<I, C> + 'static,
        I: Debug + Send + Unpin + 'static,
        C: Debug + Send + Unpin + 'static,
    {
        WebBuilder {
            client: self.client.clone(),
            logger: self.logger.clone(),
            start: None,
            handler: None,
            context: None,
            concurrent_requests: None,
            task_queue_size_bytes: None,
        }
    }
}

/// A `WebBuilder` can be used to create a [Web](Web) with custom behavior.
pub struct WebBuilder<H, C> {
    client: Client,
    logger: Logger,
    start: Option<Request>,
    handler: Option<H>,
    context: Option<C>,
    concurrent_requests: Option<NonZeroUsize>,
    task_queue_size_bytes: Option<NonZeroUsize>,
}

impl<H, C> WebBuilder<H, C>
where
    C: Debug + Send + Unpin + 'static,
{
    /// Set the initial request to be processed.
    pub fn start(mut self, start: Request) -> Self {
        self.start = Some(start);
        self
    }
    /// Set the [Handler](Handler) that processes the [Response](reqwest::Response) generated from
    /// the initial [Request](reqwest::Request).
    pub fn handler(mut self, handler: H) -> Self {
        self.handler = Some(handler);
        self
    }
    /// Set the initial context.
    pub fn context(mut self, context: C) -> Self {
        self.context = Some(context);
        self
    }
    /// Set the maximum allowed concurrent requests to be executed during a crawl.
    pub fn concurrent_requests(mut self, concurrent_requests: NonZeroUsize) -> Self {
        self.concurrent_requests = Some(concurrent_requests);
        self
    }
    /// Set the task queue size used during a crawl.
    pub fn task_queue_size_bytes(mut self, task_queue_size_bytes: NonZeroUsize) -> Self {
        self.task_queue_size_bytes = Some(task_queue_size_bytes);
        self
    }

    /// Build the `Web`.
    pub fn build<I>(self) -> Web<I, C>
    where
        I: Debug + Send + Unpin + 'static,
        H: Handler<I, C> + 'static,
    {
        let callback = Callback::new(
            self.handler.expect("initial request handler"),
            self.start.expect("initial request"),
            self.context.expect("initial context"),
        );

        Web {
            client: self.client,
            logger: self.logger,
            start: callback,
            concurrent_requests: self
                .concurrent_requests
                .unwrap_or_else(|| NonZeroUsize::new(20).unwrap()),
            task_queue_size_bytes: self
                .task_queue_size_bytes
                .unwrap_or_else(|| NonZeroUsize::new(10_000_000).unwrap()),
        }
    }
}

/// A `Web` defines how to process HTML pages.
pub struct Web<I, C> {
    client: Client,
    logger: Logger,
    start: Callback<I, C>,
    concurrent_requests: NonZeroUsize,
    task_queue_size_bytes: NonZeroUsize,
}

impl<I, C> Web<I, C>
where
    I: Debug + Send + Unpin + 'static,
    C: Debug + Send + Unpin + 'static,
{
    /// Start processing HTML pages. This method generates detached tasks upon execution.
    ///
    /// # Returns
    /// A stream of Items produced from the contents of the pages.
    pub async fn crawl(self) -> impl Stream<Item = I> {
        let concurrent_requests = self.concurrent_requests.into();
        let task_queue_size =
            self.task_queue_size_bytes.get() / std::mem::size_of::<PendingCallback<I, C>>();

        info!(&self.logger, "Starting traversal";
            "task_queue_size" => task_queue_size,
            "concurrent_requests" => concurrent_requests);

        let (item_sender, mut item_reciever) = unbounded_channel();
        let (task_sender, mut task_reciever) = channel(task_queue_size);

        let pending_start = PendingCallback {
            inner: self.start,
            task_sender: task_sender.clone(),
            item_sender,
        };

        let logger = self.logger;
        let client = self.client;
        // Load the first task
        task_sender
            .send(pending_start)
            .await
            .expect("active task channel");
        let pending_logger = logger.clone();

        // Spawn a manager task on a new thread to process the tasks
        spawn(async move {
            // Convert the reciever to a stream to increase iteration method choice
            let task_stream = async_stream::stream! {
                    while let Some(callback) = task_reciever.recv().await {
                    let client = client.clone();
                    let pending_logger = pending_logger.clone();
                    let callback_name = format!("{}", callback.inner);
                    yield spawn(async move {
                        if let Err(err) = callback.run(
                            client,
                            pending_logger.clone(),
                        )
                        .await
                        {
                            error!(pending_logger,
                           "Error occurred while executing the callback";
                           "error" => %err, "callback" => callback_name);
                        }
                    });
                }
            };
            task_stream
                .buffer_unordered(concurrent_requests)
                .for_each(move |join_handle| {
                    let execution_logger = logger.clone();
                    async move {
                        if let Err(join_err) = join_handle {
                            error!(execution_logger, "Error joining the task"; "error" => %join_err);
                        }
                    }
                })
                .await;
        });

        // Convert the reciever to a stream
        let stream = async_stream::stream! {
                while let Some(item) = item_reciever.recv().await {
                    yield item;

            }
        };

        stream
    }
}

/// An internal wrapper used primarily to control the lifespan of the associated channels.
#[derive(Debug)]
pub(crate) struct PendingCallback<I, C> {
    inner: Callback<I, C>,
    task_sender: Sender<Self>,
    item_sender: UnboundedSender<I>,
}

impl<I, C> PendingCallback<I, C>
where
    I: Debug,
    C: Debug,
{
    pub(crate) async fn run(self, client: Client, logger: Logger) -> Result<(), Error<I, C>> {
        let callback_name = format!("{}", &self.inner);
        info!(logger, "Runnning callback"; "callback" => &callback_name);
        let output = match self.inner.run(client, logger.clone()).await {
            Ok(mut stream) => {
                while let Some(indeterminate) = stream.recv().await {
                    match indeterminate {
                        Indeterminate::Item(item) => {
                            if let Err(err) = self.item_sender.send(item) {
                                crit!(logger,
                                      "Got an error sending an item";
                                      "error" => %err);
                                return Err(Error::ItemQueue(err));
                            }
                        }
                        Indeterminate::Callback(next) => {
                            let next_name = format!("{}", next);
                            let pending_next = Self {
                                inner: next,
                                task_sender: self.task_sender.clone(),
                                item_sender: self.item_sender.clone(),
                            };
                            if let Err(err) = self.task_sender.send(pending_next).await {
                                crit!(logger,
                                      "Got an error queuing the next task";
                                      "error" => %err, "next" => next_name);
                                return Err(Error::TaskQueue(err));
                            }
                        }
                    }
                }
                Ok(())
            }
            Err(err) => Err(Error::Callback(err)),
        };

        debug!(logger, "Finishing callback"; "callback" => callback_name);
        output
    }
}
