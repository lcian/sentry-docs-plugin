"use strict";
/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
exports.activateInlayHints = activateInlayHints;
const path = __importStar(require("path"));
const os = __importStar(require("os"));
const fs = __importStar(require("fs"));
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
// type a = Parameters<>;
function getServerPath(context) {
    const platform = os.platform();
    const arch = os.arch();
    // Map VSCode's architecture names to our binary architecture names
    const archMapping = {
        'arm64': 'aarch64',
        'x64': 'x86_64'
    };
    // Map VSCode's platform names to our binary platform names
    const platformMapping = {
        'darwin': 'apple-darwin',
        'win32': 'pc-windows-msvc',
        'linux': 'unknown-linux-gnu'
    };
    const mappedArch = archMapping[arch] || arch;
    const mappedPlatform = platformMapping[platform] || platform;
    const binaryName = `sentry-docs-language-server-${mappedArch}-${mappedPlatform}`;
    // Append extension for Windows
    const binaryPath = path.join(context.extensionPath, 'server', 'bin', platform === 'win32' ? `${binaryName}.exe` : binaryName);
    if (!fs.existsSync(binaryPath)) {
        throw new Error(`Language server binary not found at ${binaryPath}`);
    }
    return binaryPath;
}
async function activate(context) {
    const traceOutputChannel = vscode_1.window.createOutputChannel("Sentry Docs Language Server");
    try {
        const serverPath = getServerPath(context);
        const serverOptions = {
            run: {
                command: serverPath,
                transport: node_1.TransportKind.stdio,
                options: {
                    env: {
                        ...process.env,
                        RUST_LOG: "debug",
                    },
                }
            },
            debug: {
                command: serverPath,
                transport: node_1.TransportKind.stdio,
                options: {
                    env: {
                        ...process.env,
                        RUST_LOG: "debug",
                    },
                }
            }
        };
        const clientOptions = {
            documentSelector: [{ scheme: 'file', language: 'mdx' }],
            synchronize: {
                fileEvents: vscode_1.workspace.createFileSystemWatcher('**/*.mdx')
            },
            outputChannel: traceOutputChannel,
            markdown: {
                isTrusted: true
            },
            connectionOptions: {
                maxRestartCount: 3
            }
        };
        client = new node_1.LanguageClient('sentry-docs-language-server', 'Sentry Docs Language Server', serverOptions, clientOptions);
        await client.start();
        traceOutputChannel.appendLine('Sentry Docs Language Server started successfully');
    }
    catch (error) {
        traceOutputChannel.appendLine(`Failed to start Sentry Docs Language Server: ${error}`);
        vscode_1.window.showErrorMessage('Failed to start Sentry Docs Language Server. Please check the output channel for more details.');
    }
}
function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
function activateInlayHints(ctx) {
    const maybeUpdater = {
        hintsProvider: null,
        updateHintsEventEmitter: new vscode_1.EventEmitter(),
        async onConfigChange() {
            this.dispose();
            const event = this.updateHintsEventEmitter.event;
            // this.hintsProvider = languages.registerInlayHintsProvider(
            //   { scheme: "file", language: "nrs" },
            //   // new (class implements InlayHintsProvider {
            //   //   onDidChangeInlayHints = event;
            //   //   resolveInlayHint(hint: InlayHint, token: CancellationToken): ProviderResult<InlayHint> {
            //   //     const ret = {
            //   //       label: hint.label,
            //   //       ...hint,
            //   //     };
            //   //     return ret;
            //   //   }
            //   //   async provideInlayHints(
            //   //     document: TextDocument,
            //   //     range: Range,
            //   //     token: CancellationToken
            //   //   ): Promise<InlayHint[]> {
            //   //     const hints = (await client
            //   //       .sendRequest("custom/inlay_hint", { path: document.uri.toString() })
            //   //       .catch(err => null)) as [number, number, string][];
            //   //     if (hints == null) {
            //   //       return [];
            //   //     } else {
            //   //       return hints.map(item => {
            //   //         const [start, end, label] = item;
            //   //         let startPosition = document.positionAt(start);
            //   //         let endPosition = document.positionAt(end);
            //   //         return {
            //   //           position: endPosition,
            //   //           paddingLeft: true,
            //   //           label: [
            //   //             {
            //   //               value: `${label}`,
            //   //               // location: {
            //   //               //   uri: document.uri,
            //   //               //   range: new Range(1, 0, 1, 0)
            //   //               // }
            //   //               command: {
            //   //                 title: "hello world",
            //   //                 command: "helloworld.helloWorld",
            //   //                 arguments: [document.uri],
            //   //               },
            //   //             },
            //   //           ],
            //   //         };
            //   //       });
            //   //     }
            //   //   }
            //   // })()
            // );
        },
        onDidChangeTextDocument({ contentChanges, document }) {
            // debugger
            // this.updateHintsEventEmitter.fire();
        },
        dispose() {
            this.hintsProvider?.dispose();
            this.hintsProvider = null;
            this.updateHintsEventEmitter.dispose();
        },
    };
    vscode_1.workspace.onDidChangeConfiguration(maybeUpdater.onConfigChange, maybeUpdater, ctx.subscriptions);
    vscode_1.workspace.onDidChangeTextDocument(maybeUpdater.onDidChangeTextDocument, maybeUpdater, ctx.subscriptions);
    maybeUpdater.onConfigChange().catch(console.error);
}
//# sourceMappingURL=extension.js.map