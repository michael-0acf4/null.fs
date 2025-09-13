use std::{path::PathBuf, sync::Arc};

use crate::{
    config::{NodeConfig, NodeIdentifier},
    nullfs::{NullFs, NullFsPath, snapshot::Snapshot},
};
use actix_web::{App, HttpResponse, HttpServer, Responder, dev::ServiceRequest, web};
use actix_web_httpauth::{extractors::basic::BasicAuth, middleware::HttpAuthentication};
use serde::Deserialize;
use serde_json::json;

pub async fn verify_basic(
    req: ServiceRequest,
    _credentials: BasicAuth,
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

pub async fn index() -> impl Responder {
    HttpResponse::Ok().body("Server is up and running")
}

#[derive(Deserialize, Debug)]
pub struct CommandsParams {
    pub volume: String,
    pub node_id: String,
}

#[derive(Deserialize, Debug)]
pub struct WithPath {
    pub path: NullFsPath,
}

pub async fn commands(
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<CommandsParams>,
) -> impl Responder {
    let volume_name = params.volume.trim();
    let fs = config
        .volumes
        .iter()
        .find(|fs| fs.get_volume_name().eq(volume_name));

    if let Some(fs) = fs {
        let commands = async {
            let snapshot = Snapshot::new(fs.clone());
            let state_file = PathBuf::from(format!(".ext-state-{}.json", params.node_id));

            snapshot.capture(&state_file).await
        };

        return match commands.await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    }

    HttpResponse::BadRequest().json(json!({
        "error": format!("Volume {:?} not found", volume_name)
    }))
}

pub async fn dir(
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<WithPath>,
) -> impl Responder {
    let volume_name;
    if let Ok(volume) = params.path.volume_name() {
        volume_name = volume;
    } else {
        return HttpResponse::BadRequest().json(json!({
            "error": format!("Volume not found in {}", params.path)
        }));
    }

    let fs = config
        .volumes
        .iter()
        .find(|vol| vol.get_volume_name().eq(&volume_name));

    if let Some(fs) = fs {
        return match fs.dir(&params.path).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    }

    HttpResponse::BadRequest().json(json!({
        "error": format!("Volume {:?} not found", volume_name)
    }))
}

pub async fn hash(
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<WithPath>,
) -> impl Responder {
    let volume_name;
    if let Ok(volume) = params.path.volume_name() {
        volume_name = volume;
    } else {
        return HttpResponse::BadRequest().json(json!({
            "error": format!("Volume not found in {}", params.path)
        }));
    }

    let fs = config
        .volumes
        .iter()
        .find(|vol| vol.get_volume_name().eq(&volume_name));

    if let Some(fs) = fs {
        return match fs.hash(&params.path).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    }

    HttpResponse::BadRequest().json(json!({
        "error": format!("Volume {:?} not found", volume_name)
    }))
}

pub async fn download(
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<WithPath>,
) -> impl Responder {
    let volume_name;
    if let Ok(volume) = params.path.volume_name() {
        volume_name = volume;
    } else {
        return HttpResponse::BadRequest().json(json!({
            "error": format!("Volume not found in {}", params.path)
        }));
    }

    let fs = config
        .volumes
        .iter()
        .find(|vol| vol.get_volume_name().eq(&volume_name));

    if let Some(fs) = fs {
        return match fs.read(&params.path).await {
            // FIXME: stream
            Ok(res) => HttpResponse::Ok().body(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    }

    HttpResponse::BadRequest().json(json!({
        "error": format!("Volume {:?} not found", volume_name)
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

pub async fn run(config: &NodeConfig, identifier: &NodeIdentifier) -> eyre::Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Starting server on {addr}");

    let config = Arc::new(config.clone());
    let identifier = Arc::new(identifier.clone());
    HttpServer::new(move || {
        App::new()
            .service(
                web::scope("/v1")
                    .app_data(web::Data::new(config.clone()))
                    .app_data(web::Data::new(identifier.clone()))
                    .wrap(HttpAuthentication::basic(verify_basic))
                    .route("/commands", web::get().to(commands))
                    .route("/dir", web::get().to(dir))
                    .route("/hash", web::get().to(hash))
                    .route("/info", web::get().to(info))
                    .route("/download", web::get().to(download)),
            )
            .route("/", web::get().to(index))
    })
    .bind(addr)?
    .run()
    .await
    .map_err(|e| e.into())
}
