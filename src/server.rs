use std::{path::PathBuf, sync::Arc};

use crate::{
    config::{NodeConfig, NodeIdentifier, User},
    nullfs::{NullFs, NullFsPath, snapshot::Snapshot},
};
use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use actix_web_httpauth::extractors::basic::BasicAuth;
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

pub fn check_auth(
    auth: BasicAuth,
    volume: &str,
    config: web::Data<Arc<NodeConfig>>,
) -> Option<HttpResponse> {
    let user = User {
        name: auth.user_id().to_owned(),
        password: auth.password().map(|password| password.to_owned()),
    };

    match config.allow(volume, user) {
        Ok(is_allowed) => {
            if is_allowed {
                return None;
            }

            Some(HttpResponse::BadRequest().json(json!({
                "error": "User unauthorized"
            })))
        }
        Err(e) => Some(HttpResponse::BadRequest().json(json!({
            "error": format!("Unknown user {e}")
        }))),
    }
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
    auth: BasicAuth,
    config: web::Data<Arc<NodeConfig>>,
    params: web::Query<CommandsParams>,
) -> impl Responder {
    let volume_name = params.volume.trim();
    if let Some(bad_resp) = check_auth(auth, volume_name, config.clone()) {
        return bad_resp;
    }

    if let Some(fs) = config.find_volume(volume_name) {
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
    auth: BasicAuth,
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

    if let Some(bad_resp) = check_auth(auth, &volume_name, config.clone()) {
        return bad_resp;
    }

    if let Some(fs) = config.find_volume(&volume_name) {
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
    auth: BasicAuth,
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

    if let Some(bad_resp) = check_auth(auth, &volume_name, config.clone()) {
        return bad_resp;
    }

    if let Some(fs) = config.find_volume(&volume_name) {
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
    auth: BasicAuth,
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

    if let Some(bad_resp) = check_auth(auth, &volume_name, config.clone()) {
        return bad_resp;
    }

    if let Some(fs) = config.find_volume(&volume_name) {
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

pub async fn exists(
    auth: BasicAuth,
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

    if let Some(bad_resp) = check_auth(auth, &volume_name, config.clone()) {
        return bad_resp;
    }

    if let Some(fs) = config.find_volume(&volume_name) {
        return match fs.exists(&params.path).await {
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

pub async fn run(
    config: Arc<NodeConfig>,
    identifier: Arc<NodeIdentifier>,
    shutdown: CancellationToken,
) -> eyre::Result<()> {
    let addr = format!("{}:{}", config.address, config.port);
    tracing::info!("Starting server on {addr}");

    let server = HttpServer::new(move || {
        App::new()
            .service(
                web::scope("/v1")
                    .app_data(web::Data::new(config.clone()))
                    .app_data(web::Data::new(identifier.clone()))
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
