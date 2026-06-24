const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');
const path = require('path');
const fs = require('fs');

let client = undefined;

function findServerExecutable(context) {
    // Respect explicit user configuration first.
    const config = vscode.workspace.getConfiguration('period');
    const configured = config.get('languageServerPath');
    if (configured && fs.existsSync(configured)) {
        return configured;
    }

    const isWindows = process.platform === 'win32';
    const commandName = isWindows ? 'period.exe' : 'period';

    // Prefer the sibling compiler executable installed by the Windows installer.
    const extRoot = context.extensionPath;
    const sibling = path.join(extRoot, '..', commandName);
    if (fs.existsSync(sibling)) {
        return sibling;
    }

    // Fallback: look for the executable on PATH.
    return commandName;
}

async function startClient(context) {
    const serverExecutable = findServerExecutable(context);
    const serverOptions = {
        run: { command: serverExecutable, args: ['--lsp'], transport: TransportKind.stdio },
        debug: { command: serverExecutable, args: ['--lsp'], transport: TransportKind.stdio }
    };

    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'period' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.period')
        }
    };

    client = new LanguageClient('period', 'Period Language Server', serverOptions, clientOptions);
    await client.start();
}

function runCurrentFile(context) {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'period') {
        vscode.window.showWarningMessage('Open a .period file to run it.');
        return;
    }

    const filePath = editor.document.fileName;
    let executable = findServerExecutable(context);
    if (process.platform === 'win32') {
        executable = executable.replace(/\.exe$/i, '');
    }

    const fileArg = JSON.stringify(filePath);
    const command = executable.includes(' ')
        ? `& ${JSON.stringify(executable)} ${fileArg}`
        : `${executable} ${fileArg}`;

    const terminal = vscode.window.terminals.find(t => t.name === 'Period')
        || vscode.window.createTerminal('Period');
    terminal.show();
    terminal.sendText(command, true);
}

async function activate(context) {
    const runCommand = vscode.commands.registerCommand('period.runFile', () => {
        runCurrentFile(context);
    });
    context.subscriptions.push(runCommand);

    try {
        await startClient(context);
    } catch (err) {
        console.error('Period language server failed to start:', err);
    }
}

function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

module.exports = { activate, deactivate };
