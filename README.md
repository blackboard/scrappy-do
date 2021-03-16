# scrappy_do

This is a concurrent asynchronous webscraping framework for Rust.

# Goals

- Friendly, simple user interface
- Speed
- No `unsafe` code
- Concurrent and asynchronous processing

# About
`scrappy_do` is built around the idea of dynamically linking HTTP response handlers. The caller defines the handlers, items being scraped, and the context passed between handlers and tells the Spider where to start and the Spider takes care of scheduling the handlers and returning the items back to the caller. The caller is left to largely figure out how they want to parse HTTP responses, but a few utilies are included that I have found generally useful.

**This crate currently requires use of the nightly Rust channel to compile.**

Examples are included in the repo [here](examples/).

## Key constucts

### Library defined stucts

#### Callback

This struct defines a callback needing to be executed at some point in the future. It takes as arguments a Handler, the request to generate the response ingested by the handler, and the new context state. Care should be taken to generate the request with the included `Client` so that any relevant state (such as cookies and headers) can be correctly applied to said request.

#### Indeterminate

An `Indeterminate` is an enum that represents the possibility of an `Item` or a `Callback`. It has 2 branches, `Indeterminate::Callback` and `Indeterminate::Item`. For convenience standard conversions have been provided that allow any struct to be converted into `Indeterminate::Item` by calling the `into()` method. If you call `into()` on a `Callback` though it will be converted into the `Indeterminate::Callback`. `scrappy-do` automatically applies these conversions for the caller with the `handle` macro allowing callers to largely ignore this type, but it has been included in documentation to help with compilation errors.

### Provided macros

#### `#[handle(item = I)]`
This macro essentially just wraps the internal function logic in an asynchronous stream and sets the appropriate return type. It takes 1 argument, `item`, which is the type that is scraped.

### `wrap!(foo)`
This macro just wraps a function in concrete Handler struct with some attached metadata.

### Caller defined structs

#### Item

This struct is what the caller wants to be scraped from the response chain. It must be defined by the caller.

#### Context

This is an optional struct that allows the handlers to pass metadata along for dynamic behavior. Its contents are defined by the caller. It should only contain information that can't be/is expensive to deduce directly from the `Response`.
