#!/usr/bin/env python3
"""Exercise the installed app-server against an isolated home and project.

No user config writes, no turns/model requests. Artifacts contain the exact
requests, responses, test config files and assertions, including a server restart.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

class Server:
    def __init__(self, home: Path, output: Path, run: str):
        self.output = output
        self.run = run
        self.sequence = 0
        self.events: list[dict] = []
        self.queue: queue.Queue = queue.Queue()
        self.log = (output / f"{run}.stderr.log").open("w")
        env = os.environ.copy()
        env["CODEX_HOME"] = str(home)
        self.process = subprocess.Popen(["codex", "app-server", "--stdio"], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.log, text=True, env=env, cwd=home)
        threading.Thread(target=self.read, daemon=True).start()
        self.rpc("initialize", {"clientInfo": {"name": "gpui_config_permissions_verification", "version": "1"}, "capabilities": {"experimentalApi": True}})
        self.send({"method": "initialized"})

    def read(self):
        for line in self.process.stdout:
            value = json.loads(line)
            self.events.append({"direction": "server", "message": value})
            self.queue.put(value)

    def send(self, message: dict):
        self.events.append({"direction": "client", "message": message})
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def rpc(self, method: str, params: dict, error: str | None = None):
        self.sequence += 1
        request_id = self.sequence
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + 30
        while True:
            message = self.queue.get(timeout=max(0.01, deadline - time.monotonic()))
            if message.get("id") != request_id:
                continue
            if error:
                assert message["error"]["data"]["config_write_error_code"] == error, message
                return message["error"]
            assert "error" not in message, message
            return message["result"]

    def close(self):
        self.process.terminate()
        self.process.wait(timeout=10)
        self.log.close()
        (self.output / f"{self.run}.json").write_text(json.dumps(self.events, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("artifacts/config-permissions/real-server"))
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    home = output / "home"
    project = output / "project"
    home.mkdir(exist_ok=True)
    (project / ".codex").mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", str(project)], check=True)
    config_file = home / "config.toml"
    config_file.write_text(f'model_verbosity = "low"\nweb_search = "cached"\n[projects.{json.dumps(str(project))}]\ntrust_level = "trusted"\n')
    (project / ".codex" / "config.toml").write_text('model_verbosity = "high"\n')
    checks = []
    server = Server(home, output, "first")
    try:
        read = server.rpc("config/read", {"cwd": str(project), "includeLayers": True})
        assert read["config"]["model_verbosity"] == "high", read
        assert read["origins"]["model_verbosity"]["name"]["type"] == "project"
        layer = next(layer for layer in read["layers"] if layer["name"]["type"] == "user")
        checks.append("effective project precedence and user file version")
        project_layer = next(layer for layer in read["layers"] if layer["name"]["type"] == "project")
        server.rpc("config/batchWrite", {"filePath": str(project / ".codex" / "config.toml"), "expectedVersion": project_layer["version"], "edits": [{"keyPath": "model_verbosity", "value": "high", "mergeStrategy": "replace"}]}, "configLayerReadonly")
        checks.append("project write rejected by server; project source remains read-only")
        requirements = server.rpc("configRequirements/read", {})
        assert "requirements" in requirements
        checks.append("actual requirements read (null is valid)")
        profiles = []
        cursor = None
        while True:
            page = server.rpc("permissionProfile/list", {"cwd": str(project), "cursor": cursor, "limit": 1})
            profiles.extend(page["data"])
            cursor = page.get("nextCursor")
            if cursor is None:
                break
        assert len(profiles) >= 3
        checks.append("actual profile pagination with limit=1")
        saved = server.rpc("config/batchWrite", {"filePath": str(config_file), "expectedVersion": layer["version"], "reloadUserConfig": True, "edits": [
            {"keyPath": "model_verbosity", "value": "medium", "mergeStrategy": "replace"},
            {"keyPath": "web_search", "value": "indexed", "mergeStrategy": "replace"},
        ]})
        read = server.rpc("config/read", {"cwd": str(project), "includeLayers": True})
        assert read["config"]["web_search"] == "indexed"
        assert read["config"]["model_verbosity"] == "high"
        assert saved["version"] == next(layer["version"] for layer in read["layers"] if layer["name"]["type"] == "user")
        checks.append("atomic multi-edit save and readback, including effective override")
        # Server edit errors must leave the controlled file byte-for-byte unchanged.
        before = config_file.read_bytes()
        server.rpc("config/batchWrite", {"filePath": str(config_file), "expectedVersion": "stale", "edits": [{"keyPath": "web_search", "value": "live", "mergeStrategy": "replace"}]}, "configVersionConflict")
        assert config_file.read_bytes() == before
        server.rpc("config/batchWrite", {"filePath": str(config_file), "expectedVersion": saved["version"], "edits": [{"keyPath": "web_search", "value": "invalid", "mergeStrategy": "replace"}]}, "configValidationError")
        assert config_file.read_bytes() == before
        checks.append("version conflict and validation failure preserve file bytes")
        saved = server.rpc("config/batchWrite", {"filePath": str(config_file), "expectedVersion": saved["version"], "edits": [{"keyPath": "web_search", "value": None, "mergeStrategy": "replace"}]})
        assert "web_search" not in config_file.read_text()
        checks.append("null replace deletes an explicit value")
        # Named profile schema is validated by the real server, not client TOML parsing.
        named = server.rpc("config/batchWrite", {"filePath": str(config_file), "expectedVersion": saved["version"], "edits": [{"keyPath": "permissions.gpui-test", "value": {"extends": ":workspace"}, "mergeStrategy": "upsert"}, {"keyPath": "default_permissions", "value": ":workspace", "mergeStrategy": "replace"}]})
        profiles = server.rpc("permissionProfile/list", {"cwd": str(project), "limit": 100})["data"]
        profile = next(profile for profile in profiles if profile["id"] == "gpui-test")
        assert profile["allowed"] and profile.get("extends") in (None, ":workspace")
        checks.append("real named profile write/list; optional extends preserved as returned")
        thread = server.rpc("thread/start", {"cwd": str(project), "ephemeral": True})
        thread_id = thread["thread"]["id"]
        server.rpc("thread/settings/update", {"threadId": thread_id, "permissions": "gpui-test"})
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and not any(event["message"].get("method") == "thread/settings/updated" and event["message"].get("params", {}).get("threadId") == thread_id for event in server.events):
            time.sleep(0.01)
        observed = [event["message"]["params"]["threadSettings"] for event in server.events if event["message"].get("method") == "thread/settings/updated" and event["message"].get("params", {}).get("threadId") == thread_id]
        assert observed and observed[-1]["activePermissionProfile"]["id"] == "gpui-test", observed
        server.rpc("thread/unsubscribe", {"threadId": thread_id})
        checks.append("real settings/update and matching settings/updated, no model turn")
    finally:
        server.close()
    server = Server(home, output, "restarted")
    try:
        read = server.rpc("config/read", {"cwd": str(project), "includeLayers": True})
        assert read["config"]["model_verbosity"] == "high"
        user = next(layer for layer in read["layers"] if layer["name"]["type"] == "user")
        assert user["config"]["model_verbosity"] == "medium"
        assert "web_search" not in user["config"]
        checks.append("restart preserves on-disk values and inheritance")
    finally:
        server.close()
    (output / "results.json").write_text(json.dumps({"codex": subprocess.check_output(["codex", "--version"], text=True).strip(), "checks": checks, "userConfigPreserved": True}, ensure_ascii=False, indent=2))
    print(json.dumps({"passed": checks, "output": str(output)}, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    main()
