use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::lsp_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::{prelude::*, EnvFilter};

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

#[derive(Debug, Clone)]
enum Tag {
    Include(PathBuf),
    PlatformContent(PathBuf),
    Other,
}

impl<'a, 'b> From<Node<'a, 'b>> for Tag {
    fn from(node: Node<'_, '_>) -> Self {
        match node.tag_name().name() {
            "Include" => Self::Include(node.attribute("name").unwrap_or("").into()),
            "PlatformContent" => Self::Include(node.attribute("includePath").unwrap_or("").into()),
            _ => Self::Other,
        }
    }
}

#[tracing::instrument]
fn get_target_path(root: &PathBuf, tag: &Tag) -> PathBuf {
    let mut res = match tag {
        Tag::Include(path) => root.join("includes/").join(path),
        Tag::PlatformContent(path) => root.join("platform-includes/").join(path),
        _ => unimplemented!(),
    };
    if res.extension().is_none() {
        res.set_extension("mdx");
    }
    res
}

#[tower_lsp_server::async_trait]
impl LanguageServer for Backend {
    #[tracing::instrument]
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        let server_info = Some(ServerInfo {
            name: "sentry-docs-language-server".to_owned(),
            version: Some(
                sentry::release_name!()
                    .unwrap_or("0.1.0".into())
                    .to_string(),
            ),
        });
        let capabilities = ServerCapabilities {
            position_encoding: Some(PositionEncodingKind::UTF16),
            definition_provider: Some(OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            ..Default::default()
        };
        panic!("test");
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
        self.client
            .log_message(
                MessageType::INFO,
                format!("did_open {}", document.uri.as_str()),
            )
            .await;
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
        if !enclosing_tag.is_ok() {
            self.client
                .log_message(
                    MessageType::INFO,
                    "goto_definition called with no enclosing tag",
                )
                .await;
            return Ok(None);
        }
        let enclosing_tag = enclosing_tag.unwrap();
        let parsed = Document::parse(enclosing_tag.as_str());
        if !parsed.is_ok() {
            self.client
                .log_message(MessageType::INFO, "goto_definition called with invalid tag")
                .await;
            return Ok(None);
        }
        let parsed = parsed.unwrap();
        let root_element = parsed.root_element();
        let tag_name = root_element.tag_name();
        self.client
            .log_message(
                MessageType::INFO,
                format!("goto_definition called within {}", tag_name.name()),
            )
            .await;
        let docs_root_path = get_docs_root_path(&uri);
        let tag = root_element.into();
        let target = get_target_path(&docs_root_path, &tag);
        Ok(Some(GotoDefinitionResponse::Link(vec![LocationLink {
            target_uri: Uri::from_str(target.to_str().unwrap()).unwrap_or(uri.clone()),
            origin_selection_range: None,
            target_selection_range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            target_range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        }])))
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tracing::instrument]
fn get_docs_root_path(uri: &Uri) -> PathBuf {
    let uri = uri.as_str();
    let i = uri
        .find("sentry-docs")
        .expect("expected \"sentry-docs\" to be in the full path to the current document");
    uri[..i + "sentry-docs".len()].into()
}

#[tracing::instrument]
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
            let mut brk = false;
            for kk in (0..=jj).rev() {
                if line[kk] == b'<' {
                    k = kk as i32;
                    brk = true;
                    break;
                } else if line[kk] == b'>' && (ii as usize != i || kk != j) {
                    brk = true;
                    break;
                }
            }
            if brk {
                break;
            }
            ii -= 1;
        }
        if k != -1 {
            eprintln!("langle found at {ii}:{k}");
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
            let mut brk = false;
            if ii != i {
                jj = 0;
            }
            for kk in jj..line.len() {
                if line[kk] == b'>' {
                    brk = true;
                    k = kk as i32;
                    break;
                } else if line[kk] == b'<' && (ii as usize != i || kk != j) {
                    brk = true;
                    break;
                }
            }
            if brk {
                break;
            }
            ii += 1;
        }
        if k != -1 {
            eprintln!("rangle found at {ii}:{k}");
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

    Ok(contents[start_i..=end_i]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let j = if i + start_i == start_i { start_j } else { 0 };
            let jj = if i + start_i == end_i {
                end_j
            } else {
                line.len() - 1
            };
            String::from_utf8_lossy(&line[j..=jj]).into_owned()
        })
        .collect::<Vec<String>>()
        .join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_tag_within_tag() {
        let text: Vec<Vec<u8>> = "<Hello />"
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 0);
        assert!(tag.is_ok());
        assert_eq!(tag.unwrap(), "<Hello />");
    }

    #[test]
    fn finds_tag_within_tag_middle() {
        let text: Vec<Vec<u8>> = "<Hello />"
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 2);
        assert!(tag.is_ok());
        assert_eq!(tag.unwrap(), "<Hello />");
    }

    #[test]
    fn finds_tag_within_tag_multiple_shorter_lines() {
        let text: Vec<Vec<u8>> = r#"

<Hello />

"#
        .to_owned()
        .lines()
        .map(|line| line.to_owned().into_bytes())
        .collect();
        let tag = find_enclosing_tag(&text, 2, 0);
        assert!(tag.is_ok());
    }

    #[test]
    fn finds_tag_within_tag_spanning_multiple_lines() {
        let text: Vec<Vec<u8>> = "<Hello \n blabla='hi' \n />"
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 0);
        assert!(tag.is_ok());
        assert_eq!(tag.unwrap(), "<Hello   blabla='hi'   />");
    }

    #[test]
    fn finds_tag_within_tag_with_attibutes() {
        let text: Vec<Vec<u8>> = r#"<Hello message="world" a="b" />"#
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 0);
        assert!(tag.is_ok());
    }

    #[test]
    fn does_not_finds_tag_outside_of_tag() {
        let text: Vec<Vec<u8>> = r#"   <Hello message="world" a="b" />"#
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 0);
        assert!(tag.is_err());
    }

    #[test]
    fn does_not_finds_tag_with_other_tags() {
        let text: Vec<Vec<u8>> = r#"<A />    <Hello message="world" a="b" />"#
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 6);
        assert!(tag.is_err());
    }

    #[test]
    fn does_not_finds_tag_if_rangle_not_there() {
        let text: Vec<Vec<u8>> = r#"<Hello message="world" a="b" "#
            .to_owned()
            .lines()
            .map(|line| line.to_owned().into_bytes())
            .collect();
        let tag = find_enclosing_tag(&text, 0, 0);
        assert!(tag.is_err());
    }
}

fn main() {
    let _guard = sentry::init((
        "https://6b76fb2a5dc1164849cd797b75b6d879@o447951.ingest.us.sentry.io/4508694563782656",
        sentry::ClientOptions {
            traces_sample_rate: 1.0,
            release: sentry::release_name!(),
            send_default_pii: true,
            in_app_include: vec!["sentry_docs_language_server"],
            in_app_exclude: vec![""],
            ..sentry::ClientOptions::default()
        },
    ));
    tracing_subscriber::registry()
        .with(EnvFilter::new("hyper_util::client::legacy::pool=off"))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(sentry_tracing::layer())
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();
            let (service, socket) = LspService::new(|client| Backend {
                client,
                documents: parking_lot::RwLock::new(HashMap::new()),
            });
            Server::new(stdin, stdout, socket).serve(service).await;
        });
}
