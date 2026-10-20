use crate::{
    host::{
        Host,
        path::{HandlerPath, handler_fn},
    },
    tests::{
        CA_CERT, SERVER_CERT, SERVER_KEY, create_hello_handler, create_host,
        default_protocol_version,
    },
};
use caramelo::{expect, matchers::eq};
use deboa::{
    cert::{CertificateExt, ContentEncoding},
    request,
};
use deboa_smol::{Client, HttpBody, cert::DeboaCertificate};
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use macro_rules_attribute::apply;
use rand::random_range;
use smol_macros::test;
use vetis::{
    VetisServer as _,
    errors::{ContentError, HostError, VetisError},
    host::{Host as _, HostConfig},
    security::TlsConfig,
};

#[apply(test)]
async fn test_handler() -> Result<(), Box<dyn std::error::Error>> {
    let port = random_range(9000..=20000);

    let security_config = TlsConfig::builder()
        .ca_file(CA_CERT)
        .cert_file(SERVER_CERT)
        .key_file(SERVER_KEY)
        .build()?;

    let host_config = HostConfig::builder()
        .hostname("localhost")
        .root_directory(".")
        .protos(&[default_protocol_version()])
        .tls(security_config)
        .bind_addresses(&[(
            "0.0.0.0"
                .parse()
                .unwrap(),
            port,
        )])
        .build()?;

    let root_path = HandlerPath::builder()
        .uri("/hello")
        .handler(handler_fn(|_request, _ctx| async move {
            let response = vetis::Response::builder()
                .status(StatusCode::OK)
                .text("Hello from localhost");
            Ok(response)
        }))
        .build()?;

    let mut host = Host::new(host_config).await?;

    host.add_path(root_path);

    let mut server = crate::Vetis::builder()
        .add_host(host)
        .await?
        .build();

    server
        .start()
        .await?;

    let cert = smol::fs::read(CA_CERT).await?;
    let client = Client::builder()
        .protos(vec![default_protocol_version()])
        .certificate(DeboaCertificate::from_slice(&cert, ContentEncoding::DER))
        .prior_knowledge(true)
        .build();

    let request = request::get(format!("https://localhost:{}{}", port, "/hello"))?
        .version(default_protocol_version())
        .send_with(&client)
        .await?;

    assert_eq!(request.status(), StatusCode::OK);

    server
        .stop()
        .await?;

    Ok(())
}

#[apply(test)]
async fn test_missing_path() -> Result<(), Box<dyn std::error::Error>> {
    let port = random_range(9000..=20000);
    let host =
        create_host(create_hello_handler(), "localhost", port, CA_CERT, SERVER_CERT, SERVER_KEY)
            .await?;
    let original_request =
        Request::get(format!("https://localhost:{}/not-found", port)).body(HttpBody::empty())?;
    let request = vetis::Request::new(original_request);
    let response = host
        .route(request, None)
        .await?;
    let inner_response = response.into_inner();
    expect(inner_response.status()).to_be(eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[apply(test)]
async fn test_handler_path_matches_descendants() -> Result<(), Box<dyn std::error::Error>> {
    let host =
        create_host(create_hello_handler(), "localhost", 0, CA_CERT, SERVER_CERT, SERVER_KEY)
            .await?;
    let original_request =
        Request::get("https://localhost:0/hello/child").body(HttpBody::empty())?;
    let response = host
        .route(vetis::Request::new(original_request), None)
        .await?
        .into_inner();

    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}

#[apply(test)]
async fn test_forbidden_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let port = random_range(9000..=20000);
    let host =
        create_host(create_hello_handler(), "localhost", port, CA_CERT, SERVER_CERT, SERVER_KEY)
            .await?;
    let original_request =
        Request::get(format!("https://localhost:{}/../hello", port)).body(HttpBody::empty())?;
    let request = vetis::Request::new(original_request);
    let response = host
        .route(request, None)
        .await?;
    let inner_response = response.into_inner();
    expect(inner_response.status()).to_be(eq(StatusCode::FORBIDDEN));
    Ok(())
}

#[apply(test)]
async fn test_auth_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|_request, _ctx| async move {
        Err(VetisError::Host(HostError::Auth("Nothing".into())))
    });

    let port = random_range(9000..=20000);
    let host = create_host(handler, "localhost", port, CA_CERT, SERVER_CERT, SERVER_KEY).await?;
    let original_request =
        Request::get(format!("https://localhost:{}/hello", port)).body(HttpBody::empty())?;
    let request = vetis::Request::new(original_request);
    let response = host
        .route(request, None)
        .await?;
    let inner_response = response.into_inner();
    expect(inner_response.status()).to_be(eq(StatusCode::UNAUTHORIZED));
    Ok(())
}

#[apply(test)]
async fn test_proxy_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|_request, _ctx| async move {
        Err(VetisError::Host(HostError::Proxy("Nothing".into())))
    });

    let port = random_range(9000..=20000);
    let host = create_host(handler, "localhost", port, CA_CERT, SERVER_CERT, SERVER_KEY).await?;
    let original_request =
        Request::get(format!("https://localhost:{}/hello", port)).body(HttpBody::empty())?;
    let request = vetis::Request::new(original_request);
    let response = host
        .route(request, None)
        .await?;
    let inner_response = response.into_inner();
    expect(inner_response.status()).to_be(eq(StatusCode::BAD_GATEWAY));
    Ok(())
}

#[apply(test)]
async fn test_content_error() -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(|_request, _ctx| async move {
        Err(VetisError::Host(HostError::Content(ContentError::Forbidden)))
    });

    let port = random_range(9000..=20000);
    let host = create_host(handler, "localhost", port, CA_CERT, SERVER_CERT, SERVER_KEY).await?;
    let original_request =
        Request::get(format!("https://localhost:{}/hello", port)).body(HttpBody::empty())?;
    let request = vetis::Request::new(original_request);
    let response = host
        .route(request, None)
        .await?;
    let inner_response = response.into_inner();
    expect(inner_response.status()).to_be(eq(StatusCode::FORBIDDEN));
    Ok(())
}

async fn assert_content_error_status(
    error: ContentError,
    expected_status: StatusCode,
) -> Result<(), Box<dyn std::error::Error>> {
    let handler = handler_fn(move |_request, _ctx| {
        let error = error.clone();
        async move { Err(VetisError::Host(HostError::Content(error))) }
    });

    let host = create_host(handler, "localhost", 0, CA_CERT, SERVER_CERT, SERVER_KEY).await?;
    let original_request = Request::get("https://localhost:0/hello").body(HttpBody::empty())?;
    let response = host
        .route(vetis::Request::new(original_request), None)
        .await?
        .into_inner();

    assert_eq!(response.status(), expected_status);
    Ok(())
}

#[apply(test)]
async fn test_content_error_status_mappings() -> Result<(), Box<dyn std::error::Error>> {
    for (error, expected_status) in [
        (ContentError::Forbidden, StatusCode::FORBIDDEN),
        (ContentError::NotFound("missing".into()), StatusCode::NOT_FOUND),
        (ContentError::InvalidMetadata("invalid".into()), StatusCode::INTERNAL_SERVER_ERROR),
        (ContentError::InvalidRange("invalid".into()), StatusCode::RANGE_NOT_SATISFIABLE),
        (ContentError::ServerError("failed".into()), StatusCode::INTERNAL_SERVER_ERROR),
    ] {
        assert_content_error_status(error, expected_status).await?;
    }

    Ok(())
}

#[apply(test)]
async fn test_status_page_uses_configured_file() -> Result<(), Box<dyn std::error::Error>> {
    let host =
        create_host(create_hello_handler(), "localhost", 0, CA_CERT, SERVER_CERT, SERVER_KEY)
            .await?;
    let response = host
        .serve_status_page(StatusCode::NOT_FOUND.as_u16(), None)
        .await?
        .into_inner();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        !response
            .into_body()
            .collect()
            .await?
            .to_bytes()
            .is_empty()
    );
    Ok(())
}

#[apply(test)]
async fn test_status_page_falls_back_to_reason() -> Result<(), Box<dyn std::error::Error>> {
    let host =
        create_host(create_hello_handler(), "localhost", 0, CA_CERT, SERVER_CERT, SERVER_KEY)
            .await?;
    let response = host
        .serve_status_page(StatusCode::INTERNAL_SERVER_ERROR.as_u16(), None)
        .await?
        .into_inner();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response
            .into_body()
            .collect()
            .await?
            .to_bytes(),
        "Internal Server Error"
    );
    Ok(())
}

#[apply(test)]
async fn test_status_page_without_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let host = crate::host::Host::new(
        HostConfig::builder()
            .hostname("localhost")
            .build()?,
    )
    .await?;
    let response = host
        .serve_status_page(StatusCode::NOT_FOUND.as_u16(), None)
        .await?
        .into_inner();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[apply(test)]
async fn test_status_page_rejects_invalid_status() -> Result<(), Box<dyn std::error::Error>> {
    let host = crate::host::Host::new(
        HostConfig::builder()
            .hostname("localhost")
            .build()?,
    )
    .await?;

    assert!(
        host.serve_status_page(99, None)
            .await
            .is_err()
    );
    Ok(())
}
