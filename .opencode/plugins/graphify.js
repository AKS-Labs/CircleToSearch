// graphify OpenCode plugin
// Injects a knowledge graph reminder before the first shell command of a
// session when the graph exists.
//
// One file for both runtimes. OpenCode 2 reads `id` and `setup()` from the
// default export; OpenCode 1 (1.3.4 and later) reads `server()` from it. Each
// ignores the other's function. Nothing is imported from an opencode package:
// a plain file in .opencode/plugins/ cannot resolve one.
//
// IMPORTANT: keep the reminder string free of backticks and $(...) constructs.
// The hook prepends `echo "<reminder>" ; <cmd>` to the user's shell command;
// backticks inside the double-quoted echo trigger command substitution,
// which both corrupts tool output and silently executes the very graphify
// command we are only suggesting. Plain words render fine in opencode's TUI.
import { existsSync } from "fs";
import { join } from "path";

// ';' not '&&' — Windows PowerShell 5.1 rejects '&&' as a statement
// separator, breaking the first shell command of the session (#1646).
const REMINDER =
  'echo "[graphify] knowledge graph at graphify-out/. For focused questions, run graphify query with your question (scoped subgraph, usually much smaller than GRAPH_REPORT.md) instead of grepping raw files. Read GRAPH_REPORT.md only for broad architecture context." ; ';

const hasGraph = (directory) => existsSync(join(directory, "graphify-out", "graph.json"));

// A project and a directory above it (often ~) can each hold a copy of this
// file, and OpenCode loads both. OpenCode 2 fails the second plugin with the
// same id ("Duplicate plugin ID"), so the id carries this file's location. The
// copies share one record of who was reminded, so a command gets one reminder.
const location = [...import.meta.url].reduce((hash, ch) => (Math.imul(hash, 31) + ch.charCodeAt(0)) >>> 0, 7);
const reminded = (globalThis.__graphifyReminded ??= new Set());

export default {
  id: `graphify-${location.toString(16)}`,

  // OpenCode 2: the command tool is "shell", and one server outlives many
  // sessions, so the reminder is tracked per session.
  async setup(ctx) {
    await ctx.tool.hook("execute.before", (event) => {
      if (event.tool !== "shell" || typeof event.input?.command !== "string") return;
      if (reminded.has(event.sessionID)) return;
      if (!hasGraph(ctx.location.directory)) return;

      event.input = { ...event.input, command: REMINDER + event.input.command };
      reminded.add(event.sessionID);
    });
  },

  // OpenCode 1: the command tool is "bash", and the server lives as long as
  // the session, so the reminder is tracked per project directory.
  async server({ directory }) {
    return {
      "tool.execute.before": async (input, output) => {
        if (input.tool !== "bash" || typeof output.args?.command !== "string") return;
        if (reminded.has(directory)) return;
        if (!hasGraph(directory)) return;

        output.args.command = REMINDER + output.args.command;
        reminded.add(directory);
      },
    };
  },
};
