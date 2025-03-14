use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

use sentry::protocol::Url;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::prelude::*;

#[derive(Debug)]
struct DocumentContents {
    inner: Vec<u8>,
}

impl DocumentContents {
    fn find() {}
}

impl From<Vec<u8>> for DocumentContents {
    fn from(inner: Vec<u8>) -> Self {
        Self { inner }
    }
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: RwLock<HashMap<Uri, DocumentContents>>,
}

#[tower_lsp_server::async_trait]
impl LanguageServer for Backend {
    #[tracing::instrument]
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let guard = sentry::init((
            "https://6b76fb2a5dc1164849cd797b75b6d879@o447951.ingest.us.sentry.io/4508694563782656",
            sentry::ClientOptions {
                traces_sample_rate: 1.0,
                release: sentry::release_name!(),
                debug: true,
                ..sentry::ClientOptions::default()
            },
        ));
        Box::leak(Box::new(guard));
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(sentry_tracing::layer())
            .init();

        let server_info = Some(ServerInfo {
            name: "sentry-docs-language-server".to_owned(),
            version: Some("1.0".to_owned()),
        });
        let capabilities = ServerCapabilities {
            position_encoding: Some(PositionEncodingKind::UTF8),
            definition_provider: Some(OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            ..Default::default()
        };
        Ok(InitializeResult {
            server_info,
            capabilities,
            ..Default::default()
        })
    }

    #[tracing::instrument]
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    #[tracing::instrument]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.documents
            .write()
            .expect("poison")
            .insert(document.uri, document.text.into_bytes().into());
    }

    #[tracing::instrument]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let change = params.content_changes.into_iter().next();
        if change.is_some() {
            self.documents
                .write()
                .expect("poison")
                .insert(uri, change.unwrap().text.into_bytes().into());
        }
    }

    #[tracing::instrument]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let params = params.text_document_position_params;
        let uri = params.text_document.uri;
        let lock = self.documents.read().expect("poison");
        let contents = lock.get(&uri);
        if contents.is_none() {
            return Ok(None);
        }
        let contents = contents.unwrap();
        let i = params.position.line as usize;
        let j = params.position.character as usize;
        drop(lock);
        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(Mutex::new(HashMap::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
