# Supported files

Bonsai scans the extensions below. Parser-backed files use tree-sitter for
signatures and structure. Compact fallback files keep useful text and shape
without a language parser.

| Mode | Extensions | Languages and file types |
| --- | --- | --- |
| Tree-sitter | `.js`, `.jsx`, `.ts`, `.tsx` | JavaScript and TypeScript |
| Tree-sitter | `.py` | Python |
| Tree-sitter | `.rs` | Rust |
| Tree-sitter | `.go` | Go |
| Tree-sitter | `.java` | Java |
| Tree-sitter | `.cs` | C# |
| Tree-sitter | `.swift` | Swift |
| Tree-sitter | `.kt` | Kotlin |
| Tree-sitter | `.c`, `.h` | C |
| Tree-sitter | `.cpp`, `.hpp` | C++ |
| Compact fallback | `.m`, `.mm` | Objective-C |
| Compact fallback | `.vue`, `.svelte`, `.astro`, `.html` | Web templates |
| Compact fallback | `.md` | Markdown |
| Compact fallback | `.json`, `.yaml`, `.yml`, `.toml` | Configuration and manifests |

The exact installed parser health is shown by `bonsai doctor`. Files still obey
`.gitignore`, `.cursorignore`, include/exclude patterns, and the file-size cap.
