#[cfg(feature = "http2")]
use crate::rt::SmolExecutor;
use crate::{
    host::HostImpl,
    listener::{supported_alpns, Listener, ListenerResult},
    tls::TlsFactory,
    VetisHosts, VetisRwLock,
};
use futures_rustls::TlsAcceptor;
use http::Version;
#[cfg(feature = "http1")]
use hyper::server::conn::http1;
#[cfg(feature = "http2")]
use hyper::server::conn::http2;
use hyper_util::server::conn::auto;
use log::error;
use peekable::future::AsyncPeekable;
use smol::{Async, Task};
#[cfg(any(feature = "http1", feature = "http2"))]
use smol_hyper::rt::FuturesIo;
use std::{borrow::Cow, collections::HashMap, net::SocketAddr, sync::Arc};
use vetis::{errors::VetisError, listener::ListenerConfig, server::http::HttpService, VetisResult};

/// TCP listener
pub struct TcpListener {
    task: Option<Task<()>>,
    config: ListenerConfig,
    hosts: VetisHosts<HostImpl>,
}

impl TcpListener {
    /// Create a new listener
    ///
    /// # Arguments
    ///
    /// * `config` - A `ListenerConfig` instance containing the listener configuration.
    ///
    /// # Returns
    ///
    /// * `Self` - A new `TcpListener` instance.
    pub fn new(config: ListenerConfig) -> Self {
        Self { task: None, config, hosts: Arc::new(VetisRwLock::new(HashMap::new())) }
    }
}

impl Listener for TcpListener {
    type Host = HostImpl;

    /// Set the virtual hosts
    ///
    /// # Arguments
    ///
    /// * `hosts` - A `VetisHosts` instance containing the virtual hosts.
    fn set_hosts(&mut self, hosts: VetisHosts<HostImpl>) {
        self.hosts = hosts;
    }

    /// Listen for incoming connections
    ///
    /// # Returns
    ///
    /// * `ListenerResult<'_, ()>` - A `ListenerResult` instance containing the result of the listener.
    fn listen(&mut self) -> ListenerResult<'_, ()> {
        let future = async move {
            let addr = SocketAddr::new(
                *self
                    .config
                    .interface(),
                self.config.port(),
            );

            let listener = Async::<std::net::TcpListener>::bind(addr)
                .map_err(|e| VetisError::Bind(e.to_string()))?;

            let task = self
                .handle_connections(listener.into(), self.hosts.clone())
                .await?;

            self.task = Some(task);

            Ok(())
        };

        Box::pin(future)
    }

    /// Stop the listener
    ///
    /// # Returns
    ///
    /// * `ListenerResult<'_, ()>` - A `ListenerResult` instance containing the result of the listener.
    fn stop(&mut self) -> ListenerResult<'_, ()> {
        let future = async move {
            if let Some(task) = self.task.take() {
                task.cancel().await;
            }
            Ok(())
        };

        Box::pin(future)
    }
}

/// Decompose the TCP listener into smaller, more manageable structs
impl TcpListener {
    async fn handle_connections(
        &mut self,
        listener: smol::net::TcpListener,
        hosts: VetisHosts<HostImpl>,
    ) -> VetisResult<Task<()>> {
        let tls_config = TlsFactory::create_tls_config(hosts.clone(), supported_alpns()).await?;
        let tls_config = match tls_config {
            Some(config) => config,
            None => {
                error!("Missing TLS config");
                return Err(VetisError::Tls("Missing TLS config".to_string()));
            }
        };

        let allow_plain_connection = self
            .config
            .protos()
            .iter()
            .any(|v| *v == Version::HTTP_11 && *v == Version::HTTP_2);

        let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let future = async move {
            loop {
                let result = listener
                    .accept()
                    .await;

                let (stream, client_addr) = match result {
                    Ok(conn_info) => conn_info,
                    Err(e) => {
                        error!("Cannot accept connection: {:?}", e);
                        continue;
                    }
                };

                // TODO: Check ACL before proceeding

                let mut peekable = AsyncPeekable::from(stream);

                let mut peeked = [0; 2];
                let result = peekable
                    .peek_exact(&mut peeked)
                    .await;

                if let Err(e) = result {
                    error!("Cannot peek connection: {:?}", e);
                    continue;
                }

                let is_tls = peeked.starts_with(&[0x16, 0x03]);
                if is_tls {
                    let tls_stream = tls_acceptor
                        .accept(peekable)
                        .await;

                    let tls_stream = match tls_stream {
                        Ok(tls_stream) => tls_stream,
                        Err(e) => {
                            error!("Cannot accept connection: {:?}", e);
                            continue;
                        }
                    };

                    let alpn = &tls_stream
                        .get_ref()
                        .1
                        .alpn_protocol();
                    if let Some(alpn_code) = alpn {
                        let Cow::Borrowed(alpn_code) = String::from_utf8_lossy(alpn_code) else {
                            error!("Cannot accept connection");
                            continue;
                        };

                        match alpn_code {
                            #[cfg(feature = "http1")]
                            "http1.1" => {
                                let service = HttpService::new(hosts.clone(), client_addr);
                                smol::spawn(
                                    http1::Builder::new()
                                        .serve_connection(FuturesIo::new(tls_stream), service),
                                )
                                .detach();
                            }
                            #[cfg(feature = "http2")]
                            "h2" => {
                                let service = HttpService::new(hosts.clone(), client_addr);
                                smol::spawn(
                                    http2::Builder::new(SmolExecutor::new())
                                        .serve_connection(FuturesIo::new(tls_stream), service),
                                )
                                .detach();
                            }
                            _ => {
                                panic!("Unsupported protocol");
                            }
                        }
                    }
                } else {
                    #[cfg(any(feature = "http1", feature = "http2"))]
                    {
                        if allow_plain_connection {
                            let service = HttpService::new(hosts.clone(), client_addr);
                            smol::spawn(async move {
                                let result = auto::Builder::new(SmolExecutor::new())
                                    .serve_connection_with_upgrades(
                                        FuturesIo::new(peekable),
                                        service,
                                    )
                                    .await;
                                match result {
                                    Err(e) => {
                                        error!("Error while processing request: {}", e.to_string())
                                    }
                                    Ok(()) => {}
                                }
                            })
                            .detach();
                        }
                    }

                    #[cfg(feature = "http3")]
                    {
                        panic!("Insecure connections are only allowed with HTTP/1.1 and H2 (H2C)");
                    }
                }
            }
        };

        let task = smol::spawn(future);

        Ok(task)
    }
}
