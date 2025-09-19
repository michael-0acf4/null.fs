use crate::{
    config::{NodeConfig, NodeIdentifier},
    server::api::*,
};
use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use actix_web_httpauth::extractors::basic::BasicAuth;
use std::sync::Arc;

pub async fn index(
    auth: BasicAuth,
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<WithPath>,
) -> impl Responder {
    HttpResponse::Ok().body("Server is up and running")
}
