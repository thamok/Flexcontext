#!/usr/bin/env python3
"""Exercise the packaged executable, including freshness in one MCP process."""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import threading


def smoke(binary, version, protocol):
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        source = root / "auth.ts"
        source.write_text("function authOld() { return 'old'; }\n")
        cli = subprocess.run(
            [binary, "search", str(root), "auth", "--json"],
            check=True, capture_output=True, text=True, timeout=30,
        )
        assert json.loads(cli.stdout)["results"][0]["symbol"] == "authOld"
        with tempfile.TemporaryFile(mode="w+") as errors:
            process = subprocess.Popen(
                [binary, "serve", str(root)], stdin=subprocess.PIPE,
                stdout=subprocess.PIPE, stderr=errors, text=True,
            )
            responses = queue.Queue()

            def read():
                for line in process.stdout:
                    responses.put(line)
                responses.put(None)

            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            request_id = 0

            def call(method, params):
                nonlocal request_id
                request_id += 1
                if protocol == "2026-07-28":
                    params["_meta"] = {
                        "io.modelcontextprotocol/protocolVersion": protocol,
                        "io.modelcontextprotocol/clientCapabilities": {},
                    }
                process.stdin.write(json.dumps(dict(
                    jsonrpc="2.0", id=request_id, method=method, params=params,
                )) + "\n")
                process.stdin.flush()
                line = responses.get(timeout=30)
                assert line is not None, "MCP process exited"
                response = json.loads(line)
                assert response["id"] == request_id, response
                assert "error" not in response, response
                result = response["result"]
                assert not result.get("isError"), result
                return result

            def search():
                return call("tools/call", dict(
                    name="code_search", arguments=dict(query="auth", detail="full"),
                ))["structuredContent"]

            try:
                if protocol == "2025-06-18":
                    initialized = call("initialize", dict(
                        protocolVersion=protocol, capabilities={},
                        clientInfo=dict(name="release-smoke", version=version),
                    ))
                    assert initialized["serverInfo"]["version"] == version
                    process.stdin.write(json.dumps(dict(
                        jsonrpc="2.0", method="notifications/initialized",
                    )) + "\n")
                    process.stdin.flush()
                else:
                    call("server/discover", {})
                assert search()["results"][0]["symbol"] == "authOld"
                # Same-length edit in the same resident process; never refresh_index.
                modified_ns = source.stat().st_mtime_ns
                source.write_text("function authNew() { return 'new'; }\n")
                # Exercise a detectable same-length edit, independent of clock granularity.
                os.utime(source, ns=(modified_ns, modified_ns + 1_000_000_000))
                changed = search()
                assert changed["results"][0]["symbol"] == "authNew", changed
                assert "'new'" in changed["results"][0]["content"], changed
                assert search()["stats"]["index_reused"]
                added = root / "added.ts"
                added.write_text("function authAdded() { return true; }\n")
                assert {r["symbol"] for r in search()["results"]} == {"authNew", "authAdded"}
                added.rename(root / "renamed.ts")
                assert {r["path"] for r in search()["results"]} == {"auth.ts", "renamed.ts"}
                source.unlink()
                (root / "renamed.ts").unlink()
                assert search()["results"] == []
                process.stdin.close()
                assert process.wait(timeout=30) == 0
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=10)
                reader.join(timeout=10)
                errors.seek(0)
                diagnostic = errors.read()
                if diagnostic:
                    print(diagnostic)
    print(f"PASS: CLI and resident MCP {protocol}: edit, reuse, add, rename, delete")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("version")
    args = parser.parse_args()
    executable = str(Path(args.binary).resolve())
    actual = subprocess.check_output([executable, "--version"], text=True, timeout=30).strip()
    assert actual == f"flexcontext {args.version}", actual
    subprocess.run([executable, "--help"], check=True, stdout=subprocess.DEVNULL, timeout=30)
    for supported in ["2025-06-18", "2026-07-28"]:
        smoke(executable, args.version, supported)
