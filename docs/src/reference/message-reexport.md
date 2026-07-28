# message-reexporter

Convert an existing Message Exporters output directory to another format.

```text
message-reexporter --input <DIR> --output <DIR> [--format FORMAT]
```

## Options

| Option | Meaning |
|--------|---------|
| `--input` | Prior export directory (format auto-detected) |
| `--output` | Destination directory (must differ from input) |
| `--format` | `csv` (default), `eml`, `mbox`, `json`, `jsonl`, or `xml` |
| `--media-mode` | `disabled`, `clone`, `convert`, or `compress` |
| `--obfuscate` | Rewrite PII for sharing |
| `--obfuscate-seed` | Optional 8-hex seed (implies obfuscate) |
| `--media-max-resolution` | Compress only |
| `--media-max-fps` | Compress only |
| `--media-min-size` | Compress only |
| `--media-skip-efficient` | Compress only |

## Notes

- Input must contain exactly one detected format class. See [Re-export between formats](../reexport.md).
- End-user walkthrough: [Re-export between formats](../reexport.md).
