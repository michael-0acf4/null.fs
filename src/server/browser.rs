use crate::{
    config::{NodeConfig, NodeIdentifier, User},
    nullfs::{File, FileType, NodeKind, NullFs, NullFsPath, millis_to_utc},
    server::api::WithPath,
};
use actix_session::Session;
use actix_web::{
    HttpResponse, Responder,
    http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
    mime::{TEXT_CSS, TEXT_HTML},
    web,
};
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Debug)]
struct FileRow {
    icon: String,
    name: String,
    size: String,
    last_modified: String,
    path: NullFsPath,
    is_dir: bool,
}

impl FileRow {
    pub fn from_file(file: File) -> Self {
        Self {
            icon: match file.stat.is_dir() {
                true => "📁".to_string(),
                false => match file.file_type {
                    FileType::Image => "🖼️",
                    FileType::Video => "🎬",
                    FileType::Archive => "📦",
                    FileType::Document => "📄",
                    FileType::Text => "📝",
                    FileType::Executable | FileType::Unkown => "📄",
                }
                .to_owned(),
            },
            name: file.path.components().last().unwrap().to_owned(),
            size: match file.stat.node {
                NodeKind::File { size } => {
                    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
                    let mut size = size as f64;
                    let mut unit = 0;

                    while size >= 1024.0 && unit < UNITS.len() - 1 {
                        size /= 1024.0;
                        unit += 1;
                    }

                    if size.fract() == 0.0 {
                        format!("{}{}", size as u64, UNITS[unit])
                    } else {
                        format!("{:.1}{}", size, UNITS[unit])
                    }
                }
                NodeKind::Dir => "---".to_string(),
            },
            last_modified: {
                millis_to_utc(file.stat.modified)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            },
            path: file.path,
            is_dir: file.stat.is_dir(),
        }
    }
}

// pub async fn check_user_session(session: Session) ->  {}

pub async fn style() -> impl Responder {
    HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, TEXT_CSS))
        .body(include_str!("views/style.css"))
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct MaybeError {
    pub error: String,
}

#[derive(Deserialize)]
pub struct MaybeLogout {
    pub logout: bool,
}

pub async fn login_post(
    form: web::Form<LoginForm>,
    config: web::Data<Arc<NodeConfig>>,
    session: Session,
) -> impl Responder {
    let user = User {
        name: form.username.clone(),
        password: if form.password.trim().is_empty() {
            None
        } else {
            Some(form.password.clone())
        },
    };

    if let Some(known_user) = config.resolve_user(&user.name) {
        if *known_user == user {
            session.insert("user", &user).unwrap();

            return HttpResponse::SeeOther()
                .insert_header(("Location", "/web/browser"))
                .finish();
        }
    }

    HttpResponse::SeeOther()
        .insert_header(("Location", "/web/login?error=Unknown user"))
        .finish()
}

pub async fn login(
    config: web::Data<Arc<NodeConfig>>,
    identity: web::Data<Arc<NodeIdentifier>>,
    qerror: Option<web::Query<MaybeError>>,
    qlogout: Option<web::Query<MaybeLogout>>,
    session: Session,
) -> impl Responder {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("login", include_str!("views/login.html"))
        .expect("Failed to add raw template");

    if let Some(flag) = qlogout {
        if flag.logout {
            session.remove("user");
        }
    }

    let mut ctx = tera::Context::new();
    ctx.insert("node_name", &config.name);
    ctx.insert("node_id", &identity.uuid);
    ctx.insert("version", &env!("CARGO_PKG_VERSION"));

    ctx.insert(
        "error",
        &qerror
            .map(|e| e.error.clone())
            .unwrap_or_else(|| "".to_owned()),
    );

    HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, TEXT_HTML))
        .body(
            tera.render("login", &ctx)
                .expect("Failed to render template"),
        )
}

pub async fn browser(
    config: web::Data<Arc<NodeConfig>>,
    identity: web::Data<Arc<NodeIdentifier>>,
    params: Option<web::Query<WithPath>>,
    session: Session,
) -> impl Responder {
    let user;
    match session.get::<User>("user") {
        Ok(Some(stored_user)) => {
            user = stored_user;
            session.insert("user", &user).unwrap(); // resets TTL?
        }
        Ok(None) => {
            return HttpResponse::SeeOther()
                .insert_header(("Location", "/web/login?error=Not logged or expired"))
                .finish();
        }
        Err(_) => {
            return HttpResponse::SeeOther()
                .insert_header(("Location", "/web/login?error=Bad cookie"))
                .finish();
        }
    };

    let mut tera = tera::Tera::default();
    tera.add_raw_template("browser", include_str!("views/browser.html"))
        .expect("Failed to add raw template");
    let mut ctx = tera::Context::new();
    ctx.insert("node_name", &config.name);
    ctx.insert("node_id", &identity.uuid);
    ctx.insert("is_root", &params.is_none());
    ctx.insert("username", &user.name);
    ctx.insert("version", &env!("CARGO_PKG_VERSION"));
    ctx.insert("entries_count", &0);

    if params.is_none() {
        let allowed_volumes = config.list_allowed_volumes(&user);
        ctx.insert("entries_count", &allowed_volumes.len());
        ctx.insert("volumes", &allowed_volumes);
    } else {
        ctx.insert("volumes", &IndexSet::new() as &IndexSet<NullFsPath>);
    }

    let try_read_path = async || -> eyre::Result<Option<(String, String, Vec<u8>)>> {
        if let Some(param) = params {
            let volume = param.path.volume_name()?;
            if !config.allow(&volume, &user) {
                ctx.insert("files", &[] as &[FileRow]);
                return Ok(None);
            }

            if let Some(fs) = config.get_initialized_fs_volume(&volume).await? {
                let filename = param
                    .path
                    .components()
                    .last()
                    .cloned()
                    .ok_or_else(|| eyre::eyre!("Could not get filename"))?;

                let stats = fs.stats(&param.path).await?;
                if stats.is_file() {
                    return Ok(Some((
                        FileType::mime_from_path(&param.path),
                        filename,
                        fs.read(&param.path).await?,
                    )));
                }

                let mut list = fs.dir(&param.path).await?;
                list.sort_by_key(|f| !f.stat.is_dir());
                ctx.insert("entries_count", &list.len());

                ctx.insert(
                    "files",
                    &list
                        .into_iter()
                        .map(|file| FileRow::from_file(file))
                        .collect::<Vec<_>>(),
                );
            }
        } else {
            ctx.insert("files", &[] as &[FileRow]);
        }

        Ok(None)
    };

    match try_read_path().await {
        Ok(None) => HttpResponse::Ok()
            .insert_header((CONTENT_TYPE, TEXT_HTML))
            .body(
                tera.render("browser", &ctx)
                    .expect("Failed to render template"),
            ),
        Ok(Some((mime, filename, data))) => HttpResponse::Ok()
            .insert_header((
                CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            ))
            .insert_header((CONTENT_TYPE, mime))
            .insert_header((CONTENT_LENGTH, data.len().to_string()))
            .body(data),
        Err(e) => HttpResponse::InternalServerError()
            .insert_header((CONTENT_TYPE, TEXT_HTML))
            .body(format!("An issue has occured: {e}")),
    }
}
