"""Adapters for the real CodeGraph CLI, without expanding native source output."""
import json
import re


def normalize_explore(raw, source):
    """Read only numbered lines inside native per-file fenced source blocks.

    Explicit line numbers are checked against the snapshot. A stale, truncated,
    or fabricated line remains unlocated and cannot earn evidence credit.
    """
    records, pointers = [], []
    path, fenced = None, False
    for line in raw.splitlines():
        header = re.match(r"^\*\*`([^`]+)`\*\*", line)
        if header and not fenced:
            path = header.group(1)
            source.lines(path)  # Validate the path before attributing a pointer.
            pointers.append(path)
        elif line.startswith("```") and path is not None:
            fenced = not fenced
        elif fenced:
            numbered = re.match(r"^(\d+)\t(.*)$", line)
            if not numbered:
                continue  # Native omission markers and presentation metadata.
            number, text = int(numbered.group(1)), numbered.group(2)
            if not text.strip():
                continue
            original = source.lines(path)
            located = number if 1 <= number <= len(original) and original[number - 1] == text else None
            records.append({"path": path, "line": located, "text": text})
    return records, list(dict.fromkeys(pointers))


def normalize_context(raw, source):
    data = json.loads(raw)
    records = []
    pointers = list(data.get("relatedFiles", []))
    for block in data.get("codeBlocks", []):
        path = block["filePath"]
        pointers.append(path)
        records.extend(source.excerpt(path, block["content"], block["startLine"], block["endLine"]))
    return records, list(dict.fromkeys(pointers))
