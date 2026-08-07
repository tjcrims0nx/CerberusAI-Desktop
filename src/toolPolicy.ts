/**
 * Which MCP tools may run unattended, and which need the user to look first.
 *
 * The bundled skills-server hands the loaded model a real shell and real
 * filesystem writes. A local model — especially an abliterated one with its
 * refusal training removed — will run whatever a prompt talks it into, and MCP
 * tool arguments are model-authored text that no schema constrains beyond type.
 * So side-effecting tools are gated behind an explicit confirmation showing the
 * exact command, and read-only tools are allowed to run freely to keep the
 * assistant usable.
 *
 * Classification is by tool name, which is the only stable identifier the MCP
 * protocol gives us. Unknown names are treated as dangerous: a third-party MCP
 * server the user adds later should not be trusted by default just because we
 * have never heard of its tools.
 */

/** Tools that mutate the filesystem or execute arbitrary code. */
const DANGEROUS_TOOLS = new Set([
  "run_command",
  "write_file",
  "replace_in_file",
  "install_awesome_skill",
]);

/**
 * Tools that only read. `read_url` performs a network fetch but cannot alter
 * local state; `skill_*` tools return prompt text from a `SKILL.md` and execute
 * nothing.
 */
const SAFE_TOOLS = new Set([
  "read_file",
  "list_dir",
  "read_url",
  "list_awesome_skills",
]);

export function requiresApproval(toolName: string): boolean {
  if (DANGEROUS_TOOLS.has(toolName)) return true;
  if (SAFE_TOOLS.has(toolName)) return false;
  // Converted agent skills are inert markdown.
  if (toolName.startsWith("skill_")) return false;
  // Fail closed for anything we do not recognise.
  return true;
}

/**
 * A short, human-readable summary of what a call will do, for the confirmation
 * dialog. Falls back to compact JSON so unknown tools still show their input.
 */
export function describeToolCall(toolName: string, args: Record<string, any>): string {
  if (!args || typeof args !== "object") return "(no arguments)";
  switch (toolName) {
    case "run_command":
      return args.cwd ? `${args.command}\n\nin ${args.cwd}` : String(args.command ?? "");
    case "write_file":
      return `Write ${String(args.content ?? "").length} characters to:\n${args.path}`;
    case "replace_in_file":
      return `In ${args.path}:\n\nreplace:\n${args.targetContent}\n\nwith:\n${args.replacementContent}`;
    case "install_awesome_skill":
      return `Clone ${args.url}\nas plugin "${args.name}"`;
    default:
      try {
        return JSON.stringify(args, null, 2);
      } catch {
        return String(args);
      }
  }
}
