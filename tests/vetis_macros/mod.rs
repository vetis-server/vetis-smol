use crate::{TestResult, common::default_protocol_version};
use deboa::{
    cert::{CertificateExt as _, ContentEncoding},
    request::get,
};
use deboa_smol::{Client, cert::DeboaCertificate};
use macro_rules_attribute::apply;
use smol_macros::test;
use std::net::Ipv4Addr;
use vetis::{Response, VetisServer as _};
use vetis_macros::{http, tls};
use vetis_smol::host::path::handler_fn;

#[apply(test)]
async fn test_http_localhost() -> TestResult<()> {
    let mut server = http!(
        from_crate => vetis_smol,
        port => 60002,
        protos => &[http::Version::HTTP_11],
        allow_unsafe_conn => true,
        handler => handler_fn(
            |_req, _ctx| async move { Ok(Response::builder().text("Hello, World!")) }
        )
    )
    .await?;

    server
        .start()
        .await?;

    let client = Client::builder()
        .prior_knowledge(true)
        .build();

    let response = get("http://localhost:60002")?
        .version(http::Version::HTTP_11)
        .send_with(&client)
        .await?;

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .text()
            .await?,
        "Hello, World!"
    );

    server
        .stop()
        .await?;

    Ok(())
}

#[apply(test)]
async fn test_https() -> TestResult<()> {
    let handler =
        handler_fn(|_req, _ctx| async move { Ok(Response::builder().text("Hello, World!")) });
    let mut server = http!(
        from_crate => vetis_smol,
        hostname => "localhost",
        protos => &[default_protocol_version()],
        port => 60001,
        interface => Ipv4Addr::UNSPECIFIED,
        handler => handler,
        tls => tls! {
            cert => "certs/server.der",
            key => "certs/server.key.der",
            ca_cert => "certs/ca.der",
            client_auth => false
        }
    )
    .await?;

    server
        .start()
        .await?;

    let certificate = DeboaCertificate::from_file("certs/ca.der", ContentEncoding::DER).await?;

    let client = Client::builder()
        .certificate(certificate)
        .build();

    let response = get("https://localhost:60001")?
        .version(default_protocol_version())
        .send_with(&client)
        .await?;

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .text()
            .await?,
        "Hello, World!"
    );

    server
        .stop()
        .await?;

    Ok(())
}
