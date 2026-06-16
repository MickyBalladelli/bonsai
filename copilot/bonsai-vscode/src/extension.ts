import * as cp from 'child_process'
import * as fs from 'fs/promises'
import * as os from 'os'
import * as path from 'path'
import * as vscode from 'vscode'

import {
  buildContextPrompt,
  buildBonsaiArgs,
  buildProjectMapText,
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

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand('bonsai.generateContext', async () => {
      const generated = await generateContext(context)
      await openContextFile(generated.outputFile)
      await vscode.env.clipboard.writeText(buildContextPrompt(generated.outputFile))
      showSuccessMessage(generated, 'Prompt copied. Paste it into Copilot Chat, ChatGPT, or Codex in VS Code.')
    }),
    vscode.commands.registerCommand('bonsai.generateAndAsk', async () => {
      const generated = await generateContext(context)
      const prompt = buildContextPrompt(generated.outputFile, generated.contextText)
      await openContextFile(generated.outputFile)
      await vscode.env.clipboard.writeText(prompt)
      await openChat(prompt)
      showSuccessMessage(generated, 'Chat opened when available. Prompt also copied.')
    }),
    vscode.commands.registerCommand('bonsai.copyContext', async () => {
      const generated = await generateContext(context)
      await vscode.env.clipboard.writeText(buildContextPrompt(generated.outputFile, generated.contextText))
      showSuccessMessage(generated, 'Full context prompt copied. Paste it into Copilot Chat, ChatGPT, or Codex in VS Code.')
    }),
    vscode.commands.registerCommand('bonsai.copyProjectMap', async () => {
      const generated = await generateContext(context)
      await vscode.env.clipboard.writeText(buildProjectMapText(generated.projectMap))
      showSuccessMessage(generated, 'Project map copied.')
    }),
    vscode.commands.registerCommand('bonsai.previewProjectMap', async () => {
      const generated = await generateContext(context)
      showProjectMapPreview(context, generated)
      showSuccessMessage(generated, 'Project map preview opened.')
    }),
    vscode.commands.registerCommand('bonsai.openContext', async () => {
      const outputFile = getConfig().outputFile
      await openContextFile(outputFile)
    })
  )
}

export function deactivate() {}

async function generateContext(context: vscode.ExtensionContext): Promise<GeneratedContext> {
  const workspaceRoot = getWorkspaceRoot()
  const config = getConfig()
  const binaryPath = await resolveBinaryPath(context, config.binaryPath)
  const stdout = await runBonsai(binaryPath, buildBonsaiArgs(workspaceRoot, config))
  const contextText = await fs.readFile(config.outputFile, 'utf8')

  return {
    contextText,
    outputFile: config.outputFile,
    projectMap: extractProjectMap(contextText, config.outputFormat),
    report: parseRunReport(stdout)
  }
}

function getWorkspaceRoot(): string {
  const folder = vscode.workspace.workspaceFolders?.[0]
  if (!folder) {
    throw new Error('Open a workspace folder before running Bonsai.')
  }
  return folder.uri.fsPath
}

function getConfig(): BonsaiConfig {
  const config = vscode.workspace.getConfiguration('bonsai')
  return {
    binaryPath: config.get<string>('binaryPath', ''),
    exclude: config.get<string[]>('exclude', []),
    include: config.get<string[]>('include', []),
    level: config.get<number>('level', 2),
    maxTokens: config.get<number>('maxTokens', 12000),
    outputFile: expandHome(config.get<string>('outputFile', '/tmp/bonsai-vscode.xml')),
    outputFormat: config.get<'json' | 'xml'>('outputFormat', 'xml'),
    respectGitignore: config.get<boolean>('respectGitignore', true)
  }
}

async function resolveBinaryPath(context: vscode.ExtensionContext, configuredPath: string): Promise<string> {
  const expandedConfiguredPath = expandHome(configuredPath.trim())
  if (expandedConfiguredPath) {
    return expandedConfiguredPath
  }

  const envBinary = expandHome(process.env.BONSAI_BIN?.trim() ?? '')
  if (envBinary) {
    return envBinary
  }

  const pathBinary = await findExecutableOnPath('bonsai')
  if (pathBinary) {
    return pathBinary
  }

  const candidates = [
    path.resolve(context.extensionPath, '..', '..', 'target', 'release', 'bonsai'),
    path.join(os.homedir(), 'dev', 'bonsai-context', 'target', 'release', 'bonsai')
  ]

  for (const candidate of candidates) {
    if (await isExecutable(candidate)) {
      return candidate
    }
  }

  return 'bonsai'
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

function runBonsai(binaryPath: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = cp.spawn(binaryPath, args, {
      cwd: getWorkspaceRoot(),
      env: process.env
    })

    let stderr = ''
    let stdout = ''

    child.stdout.on('data', chunk => {
      stdout += chunk.toString()
    })

    child.stderr.on('data', chunk => {
      stderr += chunk.toString()
    })

    child.on('error', error => {
      reject(new Error(`Could not run Bonsai: ${error.message}`))
    })

    child.on('close', code => {
      if (code === 0) {
        resolve(stdout)
        return
      }
      reject(new Error(`Bonsai failed with exit code ${code}: ${stderr}`))
    })
  })
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
