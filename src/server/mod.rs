use crate::{
    config::{NodeConfig, NodeIdentifier},
    server::api::*,
};
use actix_web::{
    App, HttpResponse, HttpServer, Responder, http::header::CONTENT_TYPE, mime::TEXT_HTML, web,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

mod api;
mod browser;

pub async fn index(
    config: web::Data<Arc<NodeConfig>>,
    identifier: web::Data<Arc<NodeIdentifier>>,
) -> impl Responder {
    HttpResponse::Ok()
        .append_header((CONTENT_TYPE, TEXT_HTML))
        .body(format!(
            "<p><b>{} ({})</b> is up and running</p>",
            config.name, identifier.uuid
        ))
}

pub async fn run(
    config: Arc<NodeConfig>,
    identifier: Arc<NodeIdentifier>,
    shutdown: CancellationToken,
) -> eyre::Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Starting server on {addr}");

    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(identifier.clone()))
            .app_data(web::Data::new(config.clone()))
            .service(
                web::scope("/v1")
                    .route("/commands", web::get().to(commands))
                    .route("/dir", web::get().to(dir))
                    .route("/hash", web::get().to(hash))
                    .route("/info", web::get().to(info))
                    .route("/exists", web::get().to(exists))
                    .route("/download", web::get().to(download)),
            )
            .route("/", web::get().to(index))
    })
    .bind(addr)?
    .run();

    tokio::select! {
        _ = server => {},
        _ = shutdown.cancelled() => {}
    };

    Ok(())
}
