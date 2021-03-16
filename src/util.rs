use reqwest::{Client, Request};
use scraper::{Html, Selector};
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("select does not have a unique element")]
    NonUniqueElement,
    #[error("select does not contain any elements")]
    NoElement,
    #[error("select does not contain an element with the provide attribute (given: {0})")]
    MissingAttribute(String),
}

#[derive(Clone)]
pub struct FormField {
    name: String,
    value: String,
}

impl FormField {
    pub fn new<N: Into<String>, V: Into<String>>(name: N, value: V) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// A `FormBuilder` can be used to build a `Form` from a retrieved webpage.
pub struct FormBuilder {
    id: Option<String>,
    name: Option<String>,
    fields: Vec<FormField>,
    body: Option<Html>,
}

impl FormBuilder {
    /// Specify the form ID. This field is optional.
    pub fn id<S: Into<String>>(mut self, id: S) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Specify the form name. This field is optional.
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The response body containing the form. This is a required field.
    pub fn body(mut self, body: Html) -> Self {
        self.body = Some(body);
        self
    }

    /// Set multiple form fields. This is optional.
    pub fn fields(mut self, fields: &mut Vec<FormField>) -> Self {
        self.fields.append(fields);
        self
    }

    /// Set a single form field. Fields are optional.
    pub fn add_field(mut self, field: FormField) -> Self {
        self.fields.push(field);
        self
    }

    /// Attempt to build a `Form`. Will return `None` if the form wasn't found in the supplied
    /// body.
    pub fn build(self) -> Option<Form> {
        let body = self.body.expect("body is required to be set");
        let mut form_qualifiers = Vec::new();

        if let Some(id) = &self.id {
            form_qualifiers.push(format!(r#"id="{}""#, id));
        }

        if let Some(name) = &self.name {
            form_qualifiers.push(format!(r#"name="{}""#, name));
        }

        let form_selector =
            Selector::parse(&format!("form[{}]", form_qualifiers.join(","))[..]).unwrap();
        let field_selector = Selector::parse("input").unwrap();

        let fields: HashMap<String, String> = self
            .fields
            .into_iter()
            .map(|field| (field.name, field.value))
            .collect();

        body.select(&form_selector).next().map(|form| {
            let mut form_fields = HashMap::<String, String>::new();
            for field in form.select(&field_selector) {
                let field_value = field.value();
                let id = match field_value.attr("id") {
                    Some(id) => id,
                    None => continue,
                };
                let value = field_value.attr("value").unwrap_or("");
                form_fields.insert(id.to_string(), value.to_string());
            }
            form_fields.extend(fields);

            let path = form.value().attr("action").unwrap();
            Form {
                path: path.to_string(),
                fields: form_fields,
            }
        })
    }
}

/// Simplifies submitting forms embedded in webpage bodies.
#[derive(Debug)]
pub struct Form {
    fields: HashMap<String, String>,
    path: String,
}

impl Form {
    pub fn builder() -> FormBuilder {
        FormBuilder {
            id: None,
            name: None,
            body: None,
            fields: Vec::new(),
        }
    }

    /// Generate a `Request` from the `Form`.
    ///
    /// # Arguments
    /// - `client`: Used to generate the `Request` object.
    /// - `url`: The host that will recieve the form request upon execution.
    pub fn generate_request(&self, client: &Client, url: Url) -> Result<Request, reqwest::Error> {
        client
            .post(url.join(&self.path).unwrap().as_str())
            .form(&self.fields)
            .build()
    }
}

/// Helper method to attempt to retrieve an attibute value from a unique element contained in the
/// `Select`.
pub fn parse_attr<'element, Select: Iterator<Item = scraper::element_ref::ElementRef<'element>>>(
    select: &'element mut Select,
    attr: &str,
) -> Result<String, ParseError> {
    get_unique_element(select)
        .map(|element| {
            element
                .value()
                .attr(attr)
                .map(|value| value.to_string())
                .ok_or_else(|| ParseError::MissingAttribute(attr.to_string()))
        })
        .flatten()
}
/// Helper method to attempt to get a unique element contained in an `Iterator`.
pub fn get_unique_element<Element, I: Iterator<Item = Element>>(
    select: &mut I,
) -> Result<Element, ParseError> {
    let mut found_element = None;
    for element in select {
        match found_element {
            None => found_element = Some(element),
            Some(_) => return Err(ParseError::NonUniqueElement),
        }
    }
    match found_element {
        Some(value) => Ok(value),
        None => Err(ParseError::NoElement),
    }
}
