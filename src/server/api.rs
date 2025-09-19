use crate::{
    config::{NodeConfig, NodeIdentifier, User},
    nullfs::{NullFs, NullFsPath, any_fs::AnyFs, snapshot::Snapshot},
};
use actix_web::{HttpResponse, Responder, body::BoxBody, web};
use actix_web_httpauth::extractors::basic::BasicAuth;
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

pub fn check_auth(
    auth: BasicAuth,
    volume: &str,
    config: web::Data<Arc<NodeConfig>>,
) -> Option<HttpResponse> {
    let user = User {
        name: auth.user_id().to_owned(),
        password: auth.password().map(|password| password.to_owned()),
    };

    if config.allow(volume, user.clone()) {
        return None;
    }

    Some(HttpResponse::BadRequest().json(json!({
        "error": format!("User {:?} targetting volume {:?} unauthorized", user.name, volume)
    })))
}

pub async fn with_fs<F, Fut>(
    config: web::Data<Arc<NodeConfig>>,
    volume_name: &str,
    ff: F,
) -> HttpResponse<BoxBody>
where
    F: FnOnce(AnyFs) -> Fut,
    Fut: Future<Output = HttpResponse<BoxBody>>,
{
    match config.get_initialized_fs_volume(volume_name).await {
        Ok(Some(fs)) => ff(fs).await,
        Ok(None) => HttpResponse::BadRequest().json(json!({
            "error": format!("Volume {volume_name:?} not found")
        })),
        Err(e) => HttpResponse::BadRequest().json(json!({
            "error": format!("Could not retrieve volume {volume_name}: {e}")
        })),
    }
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
    this_node: web::Data<Arc<NodeIdentifier>>,
    params: web::Query<CommandsParams>,
) -> impl Responder {
    let volume_name = params.volume.trim();
    if let Some(bad_resp) = check_auth(auth, volume_name, config.clone()) {
        return bad_resp;
    }

    with_fs(config.clone(), volume_name, async |fs| {
        let commands = async {
            let snapshot = Snapshot::new(fs.clone());
            let state_file = PathBuf::from(format!(
                ".ext-state-{}-{}.json",
                this_node.uuid, params.node_id
            ));

            snapshot.capture(&state_file).await
        };

        return match commands.await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        };
    })
    .await
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

    with_fs(config.clone(), &volume_name, async |fs| {
        match fs.dir(&params.path).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        }
    })
    .await
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

    with_fs(config.clone(), &volume_name, async |fs| {
        match fs.hash(&params.path).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        }
    })
    .await
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

    with_fs(config.clone(), &volume_name, async |fs| {
        match fs.read(&params.path).await {
            // FIXME: stream
            Ok(res) => HttpResponse::Ok().body(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        }
    })
    .await
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

    with_fs(config.clone(), &volume_name, async |fs| {
        match fs.exists(&params.path).await {
            Ok(res) => HttpResponse::Ok().json(res),
            Err(e) => HttpResponse::InternalServerError().json(json!({
                "error": e.to_string()
            })),
        }
    })
    .await
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
