use actix_cors::Cors;
use actix_multipart::Multipart;
use actix_web::{
    get, http::header, post, web, web::Bytes, App, Error, HttpRequest, HttpResponse, HttpServer,
};
use anyhow::Result;
use futures_util::StreamExt as _;
use std::{fs::File, io::Write, path::PathBuf};

/// Save file to directory
async fn save_file(bytes: Vec<Bytes>, path: &PathBuf) -> anyhow::Result<()> {
    let path = path.clone();
    // File::create is blocking operation, use threadpool
    let mut f = web::block(|| File::create(path)).await??;

    // Field in turn is stream of *Bytes* object
    for data in bytes {
        // filesystem operations are blocking, we have to use threadpool
        f = web::block(move || f.write_all(&data).map(|_| f)).await??;
    }

    Ok(())
}

/// Train model
async fn train(path: &PathBuf) -> anyhow::Result<PathBuf> {
    Ok(path.clone())
}

/// greeting
#[post("/vc/{name}")]
async fn upload(_artist: web::Path<String>, mut payload: Multipart) -> Result<HttpResponse, Error> {
    let mut bytes: Vec<Bytes> = Vec::new();

    // iterate over multipart stream
    while let Some(item) = payload.next().await {
        let mut field = item?;

        if field.name() != "source" {
            continue;
        }

        // Field in turn is stream of *Bytes* object
        while let Some(chunk) = field.next().await {
            bytes.push(chunk?);
        }
    }

    if bytes.is_empty() {
        return Ok(HttpResponse::BadRequest().into());
    }

    let path = PathBuf::from("source.wav");
    save_file(bytes, &path).await.expect("save file error");

    let output = train(&path)
        .await
        .expect("train error")
        .to_string_lossy()
        .to_string();

    Ok(HttpResponse::Ok()
        .content_type(header::ContentType::plaintext())
        .insert_header(("X-Hdr", "sample"))
        .body(format!("/download/{output}")))
}

/// Download file
#[get("/download/{name}")]
async fn download(req: HttpRequest, audio: web::Path<String>) -> Result<HttpResponse, Error> {
    println!("download: {:?}", audio);
    let path = audio.to_string();
    let file = actix_files::NamedFile::open_async(PathBuf::from(path)).await?;
    Ok(file.into_response(&req))
}

/// start service
pub async fn start(port: u16) -> anyhow::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![header::AUTHORIZATION, header::ACCEPT])
            .allowed_header(header::CONTENT_TYPE)
            .max_age(3600);

        App::new().wrap(cors).service(upload).service(download)
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
    .map_err(Into::into)
}
