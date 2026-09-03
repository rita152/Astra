#!/usr/bin/env node

const fs = require("node:fs");

const asarPath = "/Applications/ChatGPT.app/Contents/Resources/app.asar";
const needles = process.argv.slice(2);
if (needles.length === 0) {
  needles.push(
    "context-compaction",
    "contextCompaction",
    "compactThread(",
    "pending-manual-context-compaction",
  );
}

function readAsarIndex(filePath) {
  const fd = fs.openSync(filePath, "r");
  const prefix = Buffer.alloc(16);
  fs.readSync(fd, prefix, 0, prefix.length, 0);
  const headerLength = prefix.readUInt32LE(12);
  const header = Buffer.alloc(headerLength);
  fs.readSync(fd, header, 0, header.length, 16);
  return {
    contentOffset: 16 + headerLength,
    fd,
    header: JSON.parse(header.toString("utf8")),
  };
}

function visitFiles(node, parts, result) {
  if (node.files) {
    for (const [name, child] of Object.entries(node.files)) {
      visitFiles(child, [...parts, name], result);
    }
    return;
  }
  if (!node.unpacked && Number.isFinite(node.size) && node.size > 0) {
    result.push({ path: parts.join("/"), ...node });
  }
}

const index = readAsarIndex(asarPath);
try {
  const files = [];
  visitFiles(index.header, [], files);
  const matches = [];
  for (const file of files) {
    if (!/\.(?:js|css|json|html|map)$/.test(file.path)) continue;
    const buffer = Buffer.alloc(file.size);
    fs.readSync(
      index.fd,
      buffer,
      0,
      buffer.length,
      index.contentOffset + Number(file.offset),
    );
    const source = buffer.toString("utf8");
    for (const needle of needles) {
      let position = source.indexOf(needle);
      let ordinal = 0;
      while (position >= 0 && ordinal < 20) {
        matches.push({
          file: file.path,
          needle,
          ordinal,
          position,
          snippet: source.slice(
            Math.max(0, position - 1_600),
            Math.min(source.length, position + needle.length + 4_800),
          ),
        });
        ordinal += 1;
        position = source.indexOf(needle, position + needle.length);
      }
    }
  }
  process.stdout.write(`${JSON.stringify(matches, null, 2)}\n`);
} finally {
  fs.closeSync(index.fd);
}
