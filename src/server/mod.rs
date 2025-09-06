use std::{path::PathBuf, sync::Arc};

use crate::{actors::Runner, config::NodeConfig, netfs::NetFs};
use actix::Addr;
use actix_web::{App, HttpResponse, HttpServer, Responder, dev::ServiceRequest, web};
use actix_web_httpauth::{extractors::basic::BasicAuth, middleware::HttpAuthentication};
use serde::Deserialize;
use serde_json::json;

pub async fn verify_basic(
    req: ServiceRequest,
    credentials: BasicAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    // let password_ok = credentials
    //     .password()
    //     .map_or(user_pwd.is_empty(), |pwd| pwd == user_pwd);
    // let user_ok = user_id == credentials.user_id();
    // let password_only = user_id.is_empty();
    // if (password_only && password_ok) || (user_ok && password_ok) {
    //     return Ok(req);
    // }

    // let msg = EndpointOutput::error_from_str("Bad credentials");
    // Err((
    //     actix_web::error::ErrorUnauthorized(msg.to_json_string()),
    //     req,
    // ))

    Ok(req)
}

#[derive(Deserialize, Debug)]
pub struct ListParams {
    pub volume: String,
    pub path: String,
}

pub async fn index() -> impl Responder {
    HttpResponse::Ok().body("Server is up and running")
}

pub async fn command(
    config: web::Data<Arc<NodeConfig>>,
    runner: web::Data<Addr<Runner>>,
) -> impl Responder {
    HttpResponse::Ok().json(json!("command"))
}

pub async fn list(
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<ListParams>,
) -> impl Responder {
    //http://localhost:5552/v1/list?volume=Screenshots&search=*
    let volume = config
        .volumes
        .iter()
        .find(|vol| vol.get_volume_name().eq(&params.volume.trim()));

    tracing::info!("{params:?}");
    if let Some(fs) = volume {
        return match fs.dir(&PathBuf::from(&params.path)).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    }

    HttpResponse::BadRequest().json(json!({
        "error": format!("Volume {:?} not found", params.volume)
    }))
}

pub async fn info(config: web::Data<Arc<NodeConfig>>) -> impl Responder {
    let relay_nodes = config
        .relay_nodes
        .iter()
        .map(|(k, v)| {
            json!({
                "name": k,
                "address": v.address
            })
        })
        .collect::<Vec<_>>();

    HttpResponse::Ok().json(json!({
        "name": config.name,
        "relayNodes": relay_nodes,
        "volumes": config.volumes
    }))
}

pub async fn run(config: &NodeConfig, runner: Addr<Runner>) -> eyre::Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Starting server on {addr}");

    let config = Arc::new(config.clone());
    HttpServer::new(move || {
        App::new()
            .service(
                web::scope("/v1")
                    .app_data(web::Data::new(config.clone()))
                    .app_data(web::Data::new(runner.clone()))
                    .wrap(HttpAuthentication::basic(verify_basic))
                    .route("/command", web::post().to(command))
                    .route("/list", web::get().to(list))
                    .route("/info", web::get().to(info)),
            )
            .route("/", web::get().to(index))
    })
    .bind(addr)?
    .run()
    .await
    .map_err(|e| e.into())
}
