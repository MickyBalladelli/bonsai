import * as cp from 'child_process'
import * as fs from 'fs/promises'
import * as os from 'os'
import * as path from 'path'
import * as vscode from 'vscode'

type ContextShrinkConfig = {
  binaryPath: string
  level: number
  maxTokens: number
  outputFile: string
}

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand('contextshrink.generateContext', async () => {
      const outputFile = await generateContext(context)
      await openContextFile(outputFile)
      await vscode.env.clipboard.writeText(buildCopilotPrompt(outputFile))
      vscode.window.showInformationMessage('ContextShrink context generated. Prompt copied for Copilot Chat.')
    }),
    vscode.commands.registerCommand('contextshrink.copyContext', async () => {
      const outputFile = await generateContext(context)
      const xml = await fs.readFile(outputFile, 'utf8')
      await vscode.env.clipboard.writeText(buildCopilotPrompt(outputFile, xml))
      vscode.window.showInformationMessage('ContextShrink XML copied. Paste into Copilot Chat.')
    }),
    vscode.commands.registerCommand('contextshrink.openContext', async () => {
      const outputFile = getConfig().outputFile
      await openContextFile(outputFile)
    })
  )
}

export function deactivate() {}

async function generateContext(context: vscode.ExtensionContext): Promise<string> {
  const workspaceRoot = getWorkspaceRoot()
  const config = getConfig()
  const binaryPath = await resolveBinaryPath(context, config.binaryPath)

  await runContextShrink(binaryPath, [
    workspaceRoot,
    '--max-tokens',
    String(config.maxTokens),
    '--level',
    String(config.level),
    '--output',
    'file',
    '--output-file',
    config.outputFile
  ])

  return config.outputFile
}

function getWorkspaceRoot(): string {
  const folder = vscode.workspace.workspaceFolders?.[0]
  if (!folder) {
    throw new Error('Open a workspace folder before running ContextShrink.')
  }
  return folder.uri.fsPath
}

function getConfig(): ContextShrinkConfig {
  const config = vscode.workspace.getConfiguration('contextshrink')
  return {
    binaryPath: config.get<string>('binaryPath', ''),
    level: config.get<number>('level', 2),
    maxTokens: config.get<number>('maxTokens', 12000),
    outputFile: expandHome(config.get<string>('outputFile', '/tmp/contextshrink-copilot.xml'))
  }
}

async function resolveBinaryPath(context: vscode.ExtensionContext, configuredPath: string): Promise<string> {
  const expandedConfiguredPath = expandHome(configuredPath.trim())
  if (expandedConfiguredPath) {
    return expandedConfiguredPath
  }

  const repoBinary = path.resolve(context.extensionPath, '..', '..', 'target', 'release', 'contextshrink')
  if (await isExecutable(repoBinary)) {
    return repoBinary
  }

  const homeBinary = path.join(os.homedir(), 'dev', 'bonsai', 'target', 'release', 'contextshrink')
  if (await isExecutable(homeBinary)) {
    return homeBinary
  }

  return 'contextshrink'
}

async function isExecutable(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath)
    return true
  } catch {
    return false
  }
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

function runContextShrink(binaryPath: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = cp.spawn(binaryPath, args, {
      cwd: getWorkspaceRoot(),
      env: process.env
    })

    let stderr = ''

    child.stderr.on('data', chunk => {
      stderr += chunk.toString()
    })

    child.on('error', error => {
      reject(new Error(`Could not run ContextShrink: ${error.message}`))
    })

    child.on('close', code => {
      if (code === 0) {
        resolve()
        return
      }
      reject(new Error(`ContextShrink failed with exit code ${code}: ${stderr}`))
    })
  })
}

async function openContextFile(outputFile: string): Promise<void> {
  const document = await vscode.workspace.openTextDocument(vscode.Uri.file(outputFile))
  await vscode.window.showTextDocument(document, { preview: false })
}

function buildCopilotPrompt(outputFile: string, xml?: string): string {
  if (xml) {
    return `Use this ContextShrink XML as compressed repository context, then answer my next question.\n\n${xml}`
  }

  return `Use the ContextShrink XML opened at ${outputFile} as compressed repository context, then answer my next question.`
}
