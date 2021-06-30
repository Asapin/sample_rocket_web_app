use std::io::Cursor;

use rocket::{Response, http::{ContentType, Status}, response::Responder};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CustomError {
    #[error("DB Error: {0}")]
    DatabseErr(#[source] diesel::result::Error)
}

impl From<diesel::result::Error> for CustomError {
    fn from(e: diesel::result::Error) -> Self {
        CustomError::DatabseErr(e)
    }
}

impl<'r> Responder<'r, 'static> for CustomError {
    fn respond_to(self, _request: &'r rocket::Request<'_>) -> rocket::response::Result<'static> {
        let body = format!("Diesel error: {}", self);
        let res = Response::build()
            .status(Status::InternalServerError)
            .header(ContentType::Plain)
            .sized_body(body.len(), Cursor::new(body))
            .finalize();

        Ok(res)
    }
}