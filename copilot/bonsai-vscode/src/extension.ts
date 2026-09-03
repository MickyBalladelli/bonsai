import * as cp from 'child_process'
import { createHash } from 'crypto'
import * as fs from 'fs/promises'
import * as https from 'https'
import * as os from 'os'
import * as path from 'path'
import * as vscode from 'vscode'

import {
  DEFAULT_MAX_TOKENS,
  DEFAULT_OUTPUT_FILE,
  buildContextPrompt,
  buildBonsaiArgs,
  buildProjectMapText,
  buildStatusText,
  buildSuccessMessage,
  BonsaiConfig,
  extractProjectMap,
  parseRunReport,
  ProjectMapEntry,
  RunReport
} from './bonsai'

type GeneratedContext = {
  contextText: string
  outputFile: string
  projectMap: ProjectMapEntry[]
  report: RunReport
}

let statusItem: vscode.StatusBarItem | undefined
let outputChannel: vscode.OutputChannel | undefined
let extensionVersion = 'unknown'
let cliVersion = 'unknown'
let lastBinaryPath: string | undefined

const RELEASE_DOWNLOAD_URL = 'https://github.com/mickyhq/bonsai/releases/download'
const INSTALL_URL = 'https://github.com/mickyhq/bonsai#install'

export function activate(context: vscode.ExtensionContext) {
  extensionVersion = String(context.extension.packageJSON.version ?? 'unknown')
  statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100)
  statusItem.command = 'bonsai.moreActions'
  statusItem.text = `Bonsai v${extensionVersion}: checking`
  statusItem.tooltip = 'Checking Bonsai binary health'
  statusItem.show()
  outputChannel = vscode.window.createOutputChannel('Bonsai')

  context.subscriptions.push(statusItem, outputChannel)
  void refreshHealth(context)

  registerCommand(context, 'bonsai.generateContext', async () => {
    const generated = await generateContext(context)
    await openContextFile(generated.outputFile)
    await vscode.env.clipboard.writeText(buildContextPrompt(generated.outputFile))
    showSuccessMessage(generated, 'Prompt copied. Paste it into Copilot Chat, ChatGPT, or Codex in VS Code.')
  })

  registerCommand(context, 'bonsai.generateAndAsk', async () => {
    const generated = await generateContext(context)
    const prompt = buildContextPrompt(generated.outputFile, generated.contextText)
    await openContextFile(generated.outputFile)
    await vscode.env.clipboard.writeText(prompt)
    await openChat(prompt)
    showSuccessMessage(generated, 'Chat opened when available. Prompt also copied.')
  })

  registerCommand(context, 'bonsai.copyChangedContext', async () => {
    const generated = await generateContext(context, { incremental: true })
    await vscode.env.clipboard.writeText(buildContextPrompt(generated.outputFile, generated.contextText))
    showSuccessMessage(generated, 'Changed context prompt copied.')
  })

  registerCommand(context, 'bonsai.copyContext', async () => {
    const generated = await generateContext(context)
    await vscode.env.clipboard.writeText(buildContextPrompt(generated.outputFile, generated.contextText))
    showSuccessMessage(generated, 'Full context prompt copied. Paste it into Copilot Chat, ChatGPT, or Codex in VS Code.')
  })

  registerCommand(context, 'bonsai.copyProjectMap', async () => {
    const generated = await generateContext(context)
    await vscode.env.clipboard.writeText(buildProjectMapText(generated.projectMap))
    showSuccessMessage(generated, 'Project map copied.')
  })

  registerCommand(context, 'bonsai.previewProjectMap', async () => {
    const generated = await generateContext(context)
    showProjectMapPreview(context, generated)
    showSuccessMessage(generated, 'Project map preview opened.')
  })

  registerCommand(context, 'bonsai.openContext', async () => {
    const workspaceRoot = await getWorkspaceRoot()
    await openContextFile(getConfig(workspaceRoot).outputFile)
  })

  registerCommand(context, 'bonsai.moreActions', async () => {
    await showMoreActions()
  })

  registerCommand(context, 'bonsai.setup', async () => {
    await runSetup(context)
  })
}

export function deactivate() {}

function registerCommand(
  context: vscode.ExtensionContext,
  command: string,
  handler: () => Promise<void>
): void {
  const disposable = vscode.commands.registerCommand(command, async () => {
    try {
      await handler()
    } catch (error) {
      await showCommandError(command, error)
    }
  })
  context.subscriptions.push(disposable)
}

async function showMoreActions(): Promise<void> {
  const actions: Array<vscode.QuickPickItem & { command: string }> = [
    {
      label: 'Copy Full Context Prompt',
      description: 'Copy the complete context prompt to the clipboard',
      command: 'bonsai.copyContext'
    },
    {
      label: 'Copy Project Map',
      description: 'Copy the compact project map to the clipboard',
      command: 'bonsai.copyProjectMap'
    },
    {
      label: 'Preview Project Map',
      description: 'Open the project map in a table',
      command: 'bonsai.previewProjectMap'
    },
    {
      label: 'Open Last Context',
      description: 'Open the most recent generated context file',
      command: 'bonsai.openContext'
    },
    {
      label: 'Run Setup',
      description: 'Check or download the Bonsai binary',
      command: 'bonsai.setup'
    }
  ]
  const selected = await vscode.window.showQuickPick(actions, {
    placeHolder: 'Choose a secondary Bonsai action'
  })
  if (selected) {
    await vscode.commands.executeCommand(selected.command)
  }
}

async function runSetup(context: vscode.ExtensionContext): Promise<void> {
  const binaryPath = await ensureBinary(context)
  const folders = vscode.workspace.workspaceFolders ?? []
  if (folders.length === 0) {
    vscode.window.showInformationMessage(`Bonsai is ready at ${binaryPath}. Open a workspace to check a repository.`)
    return
  }

  const workspaceRoot = await getWorkspaceRoot()
  const config = getConfig(workspaceRoot)
  const stdout = await runBonsai(
    binaryPath,
    ['setup', workspaceRoot, '--output-file', config.outputFile],
    workspaceRoot
  )
  appendOutput(stdout)
  await refreshHealth(context)
  vscode.window.showInformationMessage(`Bonsai is ready for ${workspaceRoot}.`)
}

async function generateContext(
  context: vscode.ExtensionContext,
  mode: { incremental?: boolean } = {}
): Promise<GeneratedContext> {
  const workspaceRoot = await getWorkspaceRoot()
  const config = getConfig(workspaceRoot)
  const binaryPath = await ensureBinary(context)
  const stdout = await runBonsai(
    binaryPath,
    buildBonsaiArgs(workspaceRoot, config, mode),
    workspaceRoot
  )
  const contextText = await fs.readFile(config.outputFile, 'utf8')
  const report = parseRunReport(stdout)

  const generated = {
    contextText,
    outputFile: config.outputFile,
    projectMap: extractProjectMap(contextText, config.outputFormat),
    report
  }

  updateStatus(generated, binaryPath)
  return generated
}

async function getWorkspaceRoot(): Promise<string> {
  const folders = vscode.workspace.workspaceFolders ?? []
  if (folders.length === 0) {
    throw new Error('Open a workspace folder before running Bonsai.')
  }

  if (folders.length === 1) {
    return folders[0].uri.fsPath
  }

  const items = folders.map(folder => ({
    label: folder.name,
    description: folder.uri.fsPath,
    folder
  }))
  const selected = await vscode.window.showQuickPick(items, {
    placeHolder: 'Choose the repository for Bonsai'
  })
  if (!selected) {
    throw new Error('Choose a workspace folder before running Bonsai.')
  }

  return selected.folder.uri.fsPath
}

function getConfig(workspaceRoot: string): BonsaiConfig {
  const config = vscode.workspace.getConfiguration('bonsai')
  const configuredOutputFile = expandHome(config.get<string>('outputFile', DEFAULT_OUTPUT_FILE))
  return {
    binaryPath: config.get<string>('binaryPath', ''),
    exclude: config.get<string[]>('exclude', []),
    include: config.get<string[]>('include', []),
    level: config.get<number>('level', 2),
    maxTokens: config.get<number>('maxTokens', DEFAULT_MAX_TOKENS),
    outputFile: path.isAbsolute(configuredOutputFile)
      ? configuredOutputFile
      : path.join(workspaceRoot, configuredOutputFile),
    outputFormat: config.get<'json' | 'xml'>('outputFormat', 'xml'),
    respectGitignore: config.get<boolean>('respectGitignore', true)
  }
}

async function ensureBinary(context: vscode.ExtensionContext): Promise<string> {
  const configuredPath = vscode.workspace.getConfiguration('bonsai').get<string>('binaryPath', '')
  const binaryPath = await resolveBinaryPath(context, configuredPath)
  if (binaryPath) {
    lastBinaryPath = binaryPath
    return binaryPath
  }

  const choice = await vscode.window.showErrorMessage(
    'Bonsai binary not found. Download a matching binary or set bonsai.binaryPath.',
    'Download Bonsai',
    'Open Install Guide',
    'Set Binary Path'
  )

  if (choice === 'Download Bonsai') {
    const downloadedPath = await downloadBinary(context)
    lastBinaryPath = downloadedPath
    await refreshHealth(context)
    vscode.window.showInformationMessage(`Bonsai downloaded to ${downloadedPath}.`)
    return downloadedPath
  }
  if (choice === 'Open Install Guide') {
    await vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL))
  }
  if (choice === 'Set Binary Path') {
    await vscode.commands.executeCommand('workbench.action.openSettings', 'bonsai.binaryPath')
  }

  throw new Error('Bonsai is not ready. Run Bonsai: More Actions and choose Run Setup.')
}

async function resolveBinaryPath(
  context: vscode.ExtensionContext,
  configuredPath: string
): Promise<string | undefined> {
  const expandedConfiguredPath = expandHome(configuredPath.trim())
  if (expandedConfiguredPath) {
    return (await isExecutable(expandedConfiguredPath)) ? expandedConfiguredPath : undefined
  }

  const envBinary = expandHome(process.env.BONSAI_BIN?.trim() ?? '')
  if (envBinary) {
    if (await isExecutable(envBinary)) {
      return envBinary
    }
  }

  const pathBinary = await findExecutableOnPath('bonsai')
  if (pathBinary) {
    return pathBinary
  }

  const downloadedBinary = downloadedBinaryPath(context)
  if (await isExecutable(downloadedBinary)) {
    return downloadedBinary
  }

  return undefined
}

function downloadedBinaryPath(context: vscode.ExtensionContext): string {
  const name = process.platform === 'win32' ? 'bonsai.exe' : 'bonsai'
  return path.join(context.globalStorageUri.fsPath, 'bin', name)
}

function releaseAsset(): string {
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    return 'bonsai-macos-arm64'
  }
  if (process.platform === 'linux' && process.arch === 'x64') {
    return 'bonsai-linux-x64'
  }
  throw new Error(
    `Automatic Bonsai download is not available for ${process.platform}/${process.arch}. Install Bonsai manually and set bonsai.binaryPath.`
  )
}

async function downloadBinary(context: vscode.ExtensionContext): Promise<string> {
  const asset = releaseAsset()
  const extension = vscode.extensions.getExtension(context.extension.id)
  const version = String(extension?.packageJSON.version ?? '').replace(/^v/, '')
  const releasePath = version ? `v${version}` : 'latest'
  const baseUrl = `${RELEASE_DOWNLOAD_URL}/${releasePath}`
  const [binary, checksum] = await Promise.all([
    downloadUrl(`${baseUrl}/${asset}`),
    downloadUrl(`${baseUrl}/${asset}.sha256`)
  ])
  const expectedHash = checksum.toString('utf8').trim().split(/\s+/)[0]
  const actualHash = createHash('sha256').update(binary).digest('hex')
  if (!expectedHash || expectedHash !== actualHash) {
    throw new Error(`SHA-256 verification failed for ${asset}`)
  }

  const binaryPath = downloadedBinaryPath(context)
  await fs.mkdir(path.dirname(binaryPath), { recursive: true })
  await fs.writeFile(binaryPath, binary)
  if (process.platform !== 'win32') {
    await fs.chmod(binaryPath, 0o755)
  }
  appendOutput(`Downloaded and verified ${asset} to ${binaryPath}`)
  return binaryPath
}

function downloadUrl(url: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const request = https.get(url, response => {
      const status = response.statusCode ?? 0
      const location = response.headers.location
      if (status >= 300 && status < 400 && location) {
        response.resume()
        downloadUrl(new URL(location, url).toString()).then(resolve, reject)
        return
      }
      if (status !== 200) {
        response.resume()
        reject(new Error(`Download failed with HTTP ${status}: ${url}`))
        return
      }

      const chunks: Buffer[] = []
      response.on('data', chunk => chunks.push(Buffer.from(chunk)))
      response.on('end', () => resolve(Buffer.concat(chunks)))
      response.on('error', reject)
    })
    request.setTimeout(15000, () => request.destroy(new Error(`Download timed out: ${url}`)))
    request.on('error', reject)
  })
}

async function isExecutable(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath, fs.constants.X_OK)
    return true
  } catch {
    return false
  }
}

async function findExecutableOnPath(name: string): Promise<string | undefined> {
  const pathValue = process.env.PATH ?? ''
  for (const directory of pathValue.split(path.delimiter)) {
    if (!directory) {
      continue
    }

    const candidate = path.join(directory, name)
    if (await isExecutable(candidate)) {
      return candidate
    }
  }

  return undefined
}

function expandHome(value: string): string {
  if (value === '~') {
    return os.homedir()
  }
  if (value.startsWith('~/')) {
    return path.join(os.homedir(), value.slice(2))
  }
  return value
}

function runBonsai(binaryPath: string, args: string[], cwd: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = cp.spawn(binaryPath, args, {
      cwd,
      env: process.env
    })

    appendOutput(`$ ${[binaryPath, ...args].map(formatCommandArgument).join(' ')}`)

    let stderr = ''
    let stdout = ''

    child.stdout.on('data', chunk => {
      stdout += chunk.toString()
    })

    child.stderr.on('data', chunk => {
      stderr += chunk.toString()
    })

    child.on('error', error => {
      appendOutput(`ERROR: ${error.message}`)
      reject(new Error(`Could not run Bonsai: ${error.message}`))
    })

    child.on('close', code => {
      appendOutput(stdout)
      appendOutput(stderr)
      if (code === 0) {
        resolve(stdout)
        return
      }
      reject(new Error(`Bonsai failed with exit code ${code}: ${stderr.trim() || 'no error details'}`))
    })
  })
}

function formatCommandArgument(value: string): string {
  return /\s/.test(value) ? JSON.stringify(value) : value
}

function appendOutput(value: string): void {
  const text = value.trim()
  if (text) {
    outputChannel?.appendLine(text)
  }
}

async function showCommandError(command: string, error: unknown): Promise<void> {
  const message = error instanceof Error ? error.message : String(error)
  appendOutput(`ERROR in ${command}: ${message}`)
  setHealthStatus('error', `Bonsai extension v${extensionVersion}\n${message}`)
  const choice = await vscode.window.showErrorMessage(
    `Bonsai failed: ${message}`,
    'Show Bonsai Output'
  )
  if (choice === 'Show Bonsai Output') {
    outputChannel?.show(true)
  }
}

async function openContextFile(outputFile: string): Promise<void> {
  const document = await vscode.workspace.openTextDocument(vscode.Uri.file(outputFile))
  await vscode.window.showTextDocument(document, { preview: false })
}

async function openChat(prompt: string): Promise<void> {
  try {
    await vscode.commands.executeCommand('workbench.action.chat.open', prompt)
  } catch {
    await vscode.commands.executeCommand('workbench.action.chat.open')
  }
}

function showSuccessMessage(generated: GeneratedContext, nextStep: string): void {
  vscode.window.showInformationMessage(buildSuccessMessage(generated.outputFile, generated.report, nextStep))
}

async function refreshHealth(context: vscode.ExtensionContext): Promise<void> {
  if (!statusItem) {
    return
  }

  setHealthStatus('checking', `Bonsai extension v${extensionVersion}\nChecking for the CLI binary`)
  const configuredPath = vscode.workspace.getConfiguration('bonsai').get<string>('binaryPath', '')
  const binaryPath = await resolveBinaryPath(context, configuredPath)
  if (!binaryPath) {
    lastBinaryPath = undefined
    cliVersion = 'unknown'
    setHealthStatus(
      'setup needed',
      `Bonsai extension v${extensionVersion}\nCLI binary not found\nRun Bonsai: More Actions > Run Setup`
    )
    return
  }

  lastBinaryPath = binaryPath
  try {
    cliVersion = await readBinaryVersion(binaryPath)
    setHealthStatus(
      'ready',
      `Bonsai extension v${extensionVersion}\nCLI ${cliVersion}\nBinary: ${binaryPath}`
    )
  } catch (error) {
    cliVersion = 'unknown'
    const message = error instanceof Error ? error.message : String(error)
    appendOutput(`Health check failed: ${message}`)
    setHealthStatus(
      'check failed',
      `Bonsai extension v${extensionVersion}\nBinary: ${binaryPath}\n${message}`
    )
  }
}

function readBinaryVersion(binaryPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    cp.execFile(binaryPath, ['--version'], { env: process.env, timeout: 5000 }, (error, stdout, stderr) => {
      if (error) {
        reject(new Error(stderr.trim() || error.message))
        return
      }

      const version = stdout.trim().split(/\r?\n/)[0]
      if (!version) {
        reject(new Error('Bonsai returned no version'))
        return
      }
      resolve(version)
    })
  })
}

function setHealthStatus(state: string, tooltip: string): void {
  if (!statusItem) {
    return
  }
  statusItem.text = `Bonsai v${extensionVersion}: ${state}`
  statusItem.tooltip = tooltip
  statusItem.show()
}

function updateStatus(generated: GeneratedContext, binaryPath: string): void {
  if (!statusItem) {
    return
  }

  lastBinaryPath = binaryPath
  const reportText = buildStatusText(generated.report).replace(/^Bonsai:\s*/, '')
  statusItem.text = `Bonsai v${extensionVersion}: ${reportText}`
  statusItem.tooltip = [
    `Bonsai extension v${extensionVersion}`,
    `CLI ${cliVersion}`,
    `Health: ${lastBinaryPath ? 'ready' : 'unknown'}`,
    `Binary: ${lastBinaryPath ?? 'not found'}`,
    `Output: ${generated.outputFile}`
  ].join('\n')
  statusItem.show()
}

function showProjectMapPreview(context: vscode.ExtensionContext, generated: GeneratedContext): void {
  const panel = vscode.window.createWebviewPanel(
    'bonsaiProjectMap',
    'Bonsai Project Map',
    vscode.ViewColumn.Beside,
    { enableScripts: false }
  )
  panel.webview.html = buildProjectMapHtml(generated)
  context.subscriptions.push(panel)
}

function buildProjectMapHtml(generated: GeneratedContext): string {
  const rows = generated.projectMap
    .map(entry => `<tr><td>${escapeHtml(entry.path)}</td><td>${entry.level}</td><td>${entry.tokens}</td></tr>`)
    .join('')

  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; padding: 16px; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border-bottom: 1px solid #ddd; padding: 6px 8px; text-align: left; }
    th { position: sticky; top: 0; background: var(--vscode-editor-background); }
    td:nth-child(2), td:nth-child(3) { text-align: right; white-space: nowrap; }
  </style>
</head>
<body>
  <h1>Bonsai Project Map</h1>
  <p>${escapeHtml(generated.outputFile)}</p>
  <table>
    <thead><tr><th>Path</th><th>Level</th><th>Tokens</th></tr></thead>
    <tbody>${rows}</tbody>
  </table>
</body>
</html>`
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}
