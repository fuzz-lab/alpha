use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{
    get, http::header, post, web, web::Bytes, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use anyhow::Result;
use futures_util::StreamExt as _;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::{fs::File, io::Write, path::PathBuf, process::Command};

enum Artist {
    Cat,
    MaoBuYi,
    WangFei,
    DuiZhang,
    XiaoXiao,
}

impl Artist {
    /// Get model path
    pub fn model(&self) -> &'static str {
        match self {
            Artist::Cat => "models/cat/G_2875.pth",
            Artist::MaoBuYi => "models/maobuyi/G_3458.pth",
            Artist::WangFei => "models/wf/G_4788.pth",
            Artist::DuiZhang => "models/dz/G_5229.pth",
            Artist::XiaoXiao => "models/xx/G_2199.pth",
        }
    }

    /// Get config path
    pub fn config(&self) -> &'static str {
        match self {
            Artist::Cat => "models/cat/config.json",
            Artist::MaoBuYi => "models/maobuyi/config.json",
            Artist::WangFei => "models/wf/config.json",
            Artist::DuiZhang => "models/dz/config.json",
            Artist::XiaoXiao => "models/xx/config.json",
        }
    }
}

impl From<&str> for Artist {
    fn from(s: &str) -> Artist {
        match s {
            "cat" => Artist::Cat,
            "mb" => Artist::MaoBuYi,
            "wf" => Artist::WangFei,
            "dz" => Artist::DuiZhang,
            "xx" => Artist::XiaoXiao,
            _ => unreachable!("Unsupported artist"),
        }
    }
}

/// Save file to directory
async fn save_file(bytes: Vec<Bytes>, path: PathBuf) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from("upload").join(path);
    let cloned_path = path.clone();

    // File::create is blocking operation, use threadpool
    log::trace!("saving file at {:?}", cloned_path);
    let mut f = web::block(|| File::create(cloned_path)).await??;

    // Field in turn is stream of *Bytes* object
    for data in bytes {
        // filesystem operations are blocking, we have to use threadpool
        f = web::block(move || f.write_all(&data).map(|_| f)).await??;
    }

    Ok(path)
}

/// Train model
async fn train(path: &PathBuf, artist: impl Into<Artist>) -> anyhow::Result<PathBuf> {
    let artist = artist.into();

    let mut args = ["infer", "-m", artist.model(), "-c", artist.config()]
        .into_iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let input = path.to_string_lossy().to_string();
    let output = input.replace("upload", "download");
    args.push("-o".into());
    args.push(output.clone());
    args.push(input);

    log::trace!("saving output at {:?}", output);

    Command::new("svc")
        .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
        .args(args)
        .status()?;
    Ok(output.into())
}

/// greeting
#[post("/vc/{name}")]
async fn upload(artist: web::Path<String>, mut payload: Multipart) -> Result<HttpResponse, Error> {
    let mut bytes: Vec<Bytes> = Vec::new();
    let mut filename = String::new();

    // iterate over multipart stream
    while let Some(item) = payload.next().await {
        let mut field = item?;

        if field.name() != "source" && field.name() != "name" {
            continue;
        }

        // Field in turn is stream of *Bytes* object
        while let Some(chunk) = field.next().await {
            let chunk = chunk?;
            if field.name() == "source" {
                bytes.push(chunk);
            } else if field.name() == "name" {
                filename += String::from_utf8_lossy(&chunk.to_vec()).as_ref();
            }
        }
    }

    if bytes.is_empty() {
        return Ok(HttpResponse::BadRequest().into());
    }

    filename = format!("{}_{filename}", artist.as_str());
    let input = save_file(bytes, PathBuf::from(filename))
        .await
        .expect("save file error");

    let output = train(&input, artist.to_string().as_ref())
        .await
        .expect("train error")
        .to_string_lossy()
        .to_string();

    Ok(HttpResponse::Ok()
        .content_type(header::ContentType::plaintext())
        .insert_header(("X-Hdr", "sample"))
        .body(format!("/{output}")))
}

/// Download file
#[get("/download/{name}")]
async fn download(req: HttpRequest, audio: web::Path<String>) -> Result<HttpResponse, Error> {
    println!("download: {:?}", audio);
    let path = audio.to_string();
    let file =
        actix_files::NamedFile::open_async(PathBuf::from(format!("download/{path}"))).await?;
    Ok(file.into_response(&req))
}

/// start service
pub async fn start(port: u16) -> anyhow::Result<()> {
    let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    builder.set_private_key_file("key.pem", SslFiletype::PEM)?;
    builder.set_certificate_chain_file("cert.pem")?;

    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
            .allowed_header(header::CONTENT_TYPE)
            .max_age(3600);

        App::new().wrap(cors).service(upload).service(download)
    })
    .bind_openssl("127.0.0.1:8080", builder)?
    .run()
    .await
    .map_err(Into::into)
}
