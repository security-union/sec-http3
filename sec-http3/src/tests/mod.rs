mod connection;
mod request;

use std::{
    convert::TryInto,
    net::{Ipv6Addr, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use rustls::{Certificate, PrivateKey};

use crate::sec_http3_quinn::{quinn::TransportConfig, Connection};
use crate::{quic, sec_http3_quinn};

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
        .with_test_writer()
        .try_init();
}

#[derive(Clone)]
pub struct Pair {
    port: u16,
    cert: Certificate,
    key: PrivateKey,
    config: Arc<TransportConfig>,
}

impl Default for Pair {
    fn default() -> Self {
        let (cert, key) = build_certs();
        Self {
            cert,
            key,
            port: 0,
            config: Arc::new(TransportConfig::default()),
        }
    }
}

impl Pair {
    pub fn with_timeout(&mut self, duration: Duration) {
        Arc::get_mut(&mut self.config)
            .unwrap()
            .max_idle_timeout(Some(
                duration.try_into().expect("idle timeout duration invalid"),
            ))
            .initial_rtt(Duration::from_millis(10));
    }

    pub fn server_inner(&mut self) -> sec_http3_quinn::Endpoint {
        let mut crypto = rustls::ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![self.cert.clone()], self.key.clone())
            .unwrap();
        crypto.max_early_data_size = u32::MAX;
        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let mut server_config = sec_http3_quinn::quinn::ServerConfig::with_crypto(Arc::new(crypto));
        server_config.transport = self.config.clone();
        let endpoint =
            sec_http3_quinn::quinn::Endpoint::server(server_config, "[::]:0".parse().unwrap())
                .unwrap();

        self.port = endpoint.local_addr().unwrap().port();

        endpoint
    }

    pub fn server(&mut self) -> Server {
        let endpoint = self.server_inner();
        Server { endpoint }
    }

    pub async fn client_inner(&self) -> quinn::Connection {
        let addr = (Ipv6Addr::LOCALHOST, self.port)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();

        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.add(&self.cert).unwrap();
        let mut crypto = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        crypto.enable_early_data = true;
        crypto.alpn_protocols = vec![b"h3".to_vec()];

        let client_config = sec_http3_quinn::quinn::ClientConfig::new(Arc::new(crypto));

        let mut client_endpoint =
            sec_http3_quinn::quinn::Endpoint::client("[::]:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(client_config);
        client_endpoint
            .connect(addr, "localhost")
            .unwrap()
            .await
            .unwrap()
    }

    pub async fn client(&self) -> sec_http3_quinn::Connection {
        Connection::new(self.client_inner().await)
    }
}

pub struct Server {
    pub endpoint: sec_http3_quinn::Endpoint,
}

impl Server {
    pub async fn next(&mut self) -> impl quic::Connection<Bytes> {
        Connection::new(self.endpoint.accept().await.unwrap().await.unwrap())
    }
}

pub fn build_certs() -> (Certificate, PrivateKey) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = PrivateKey(cert.serialize_private_key_der());
    let cert = Certificate(cert.serialize_der().unwrap());
    (cert, key)
}
