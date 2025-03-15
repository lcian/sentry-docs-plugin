use roxmltree::Document;
use std::collections::HashMap;

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::prelude::*;

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: parking_lot::RwLock<HashMap<Uri, Vec<Vec<u8>>>>,
}

impl Backend {
    #[tracing::instrument]
    async fn update_document(&self, uri: Uri, text: String) {
        self.documents.write().insert(
            uri,
            text.lines()
                .map(|line| line.to_owned().into_bytes())
                .collect(),
        );
    }
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
            version: Some(
                sentry::release_name!()
                    .unwrap_or("0.1.0".into())
                    .to_string(),
            ),
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
        self.update_document(document.uri, document.text).await;
    }

    #[tracing::instrument]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let change = params.content_changes.into_iter().next();
        if change.is_some() {
            self.update_document(uri, change.unwrap().text).await;
        }
    }

    #[tracing::instrument]
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let params = params.text_document_position_params;
        let uri = params.text_document.uri;
        let enclosing_tag = {
            let map = self.documents.read();
            let contents = map.get(&uri);
            if contents.is_none() {
                return Ok(None);
            }
            let contents = contents.unwrap();
            let i = params.position.line as usize;
            let j = params.position.character as usize;
            find_enclosing_tag(contents, i, j)
        };
        if enclosing_tag.is_ok() {
            return Ok(None);
        }
        let enclosing_tag = enclosing_tag.unwrap();
        let parsed = Document::parse(enclosing_tag.as_str()).expect("failed to parse tag");
        let tag_name = parsed.root_element().tag_name();
        self.client
            .log_message(MessageType::INFO, tag_name.name().to_string())
            .await;
        Ok(None)
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

fn find_enclosing_tag(contents: &[Vec<u8>], i: usize, j: usize) -> std::result::Result<String, ()> {
    let langle_pos = {
        let mut ii = i as i32;
        let mut jj = j;
        let mut k = -1_i32;
        while ii >= 0 {
            let line = &contents[ii as usize];
            if ii as usize != i {
                jj = line.len() - 1;
            }
            if let Some(kk) = line.iter().take(jj + 1).rposition(|c| *c == b'<') {
                k = kk as i32;
                break;
            }
            ii -= 1;
        }
        if k != -1 {
            Some((ii as usize, k as usize))
        } else {
            None
        }
    };
    let rangle_pos = {
        let mut ii = i;
        let mut jj = j;
        let mut k = -1_i32;
        while ii < contents.len() {
            let line = &contents[ii];
            if ii != i {
                jj = 0;
            }
            if let Some(kk) = line.iter().take(jj + 1).position(|c| *c == b'>') {
                k = kk as i32;
                break;
            }
            ii += 1;
        }
        if k != -1 {
            Some((ii, k as usize))
        } else {
            None
        }
    };
    if !(langle_pos.is_some() && rangle_pos.is_some()) {
        return Err(());
    }

    let (start_i, start_j) = langle_pos.unwrap();
    let (end_i, end_j) = rangle_pos.unwrap();

    Ok(contents[start_i..end_i]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let j = if i == start_i { start_j } else { 0 };
            let jj = if i == end_i { end_j } else { line.len() - 1 };
            String::from_utf8_lossy(&contents[i][j..jj]).into_owned()
        })
        .collect::<Vec<String>>()
        .join(" "))
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: parking_lot::RwLock::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
