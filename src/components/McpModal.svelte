<script lang="ts">
  import { Bot, Check, Copy, Terminal, Trash2, X } from '@lucide/svelte';
  import { getMcpClientStatuses, installMcpClient, uninstallMcpClient } from '../lib/ipc';
  import type { McpClientStatus } from '../lib/types';

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  let statuses = $state<McpClientStatus[]>([]);
  let loading = $state(false);
  let actionMessage = $state<string | null>(null);
  let activeTab = $state<'local' | 'chatgpt' | 'manual'>('local');
  let copiedSnippet = $state(false);

  const manualSnippet = `{
  "mcpServers": {
    "fossil": {
      "command": "fossil",
      "args": ["mcp"]
    }
  }
}`;

  async function loadStatuses() {
    loading = true;
    try {
      statuses = await getMcpClientStatuses();
    } catch (err) {
      console.error('Failed to load MCP client statuses:', err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      loadStatuses();
      actionMessage = null;
    }
  });

  async function handleInstall(key: string) {
    loading = true;
    actionMessage = null;
    try {
      const msg = await installMcpClient(key);
      actionMessage = msg;
      await loadStatuses();
    } catch (err) {
      actionMessage = `Error: ${String(err)}`;
    } finally {
      loading = false;
    }
  }

  async function handleUninstall(key: string) {
    loading = true;
    actionMessage = null;
    try {
      const msg = await uninstallMcpClient(key);
      actionMessage = msg;
      await loadStatuses();
    } catch (err) {
      actionMessage = `Error: ${String(err)}`;
    } finally {
      loading = false;
    }
  }

  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedSnippet = true;
      setTimeout(() => {
        copiedSnippet = false;
      }, 2000);
    } catch (e) {
      console.error('Failed to copy', e);
    }
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
    <!-- Backdrop -->
    <div
      class="fixed inset-0 bg-black/70 backdrop-blur-xs transition-opacity"
      onclick={onclose}
      role="presentation"
    ></div>

    <!-- Modal Dialog -->
    <div
      class="relative flex max-h-[85vh] w-full max-w-2xl flex-col rounded-2xl border border-zinc-800 bg-zinc-950 p-6 shadow-2xl"
      role="dialog"
      aria-modal="true"
    >
      <!-- Header -->
      <div class="flex items-start justify-between pb-4 border-b border-zinc-800">
        <div class="flex items-center gap-3">
          <div
            class="flex h-10 w-10 items-center justify-center rounded-xl bg-purple-500/10 text-purple-400 border border-purple-500/20"
          >
            <Bot class="h-5 w-5" />
          </div>
          <div>
            <h2 class="text-base font-semibold text-zinc-100">Connect to AI Assistants (MCP)</h2>
            <p class="text-xs text-zinc-400">
              Let Gemini, Claude, ChatGPT, and Codex inspect storage, clean bloat, and find space
              hogs safely.
            </p>
          </div>
        </div>
        <button
          onclick={onclose}
          class="cursor-pointer rounded-lg p-1.5 text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200 transition"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- Navigation Tabs -->
      <div class="flex gap-2 pt-4 pb-3 border-b border-zinc-800/80 text-xs font-medium">
        <button
          onclick={() => (activeTab = 'local')}
          class="cursor-pointer rounded-lg px-3 py-1.5 transition {activeTab === 'local'
            ? 'bg-zinc-800 text-white font-semibold'
            : 'text-zinc-400 hover:text-zinc-200'}"
        >
          1-Click Client Setup
        </button>
        <button
          onclick={() => (activeTab = 'chatgpt')}
          class="cursor-pointer rounded-lg px-3 py-1.5 transition {activeTab === 'chatgpt'
            ? 'bg-zinc-800 text-white font-semibold'
            : 'text-zinc-400 hover:text-zinc-200'}"
        >
          ChatGPT Connectors
        </button>
        <button
          onclick={() => (activeTab = 'manual')}
          class="cursor-pointer rounded-lg px-3 py-1.5 transition {activeTab === 'manual'
            ? 'bg-zinc-800 text-white font-semibold'
            : 'text-zinc-400 hover:text-zinc-200'}"
        >
          Manual Configuration
        </button>
      </div>

      <!-- Content Area -->
      <div class="flex-1 overflow-y-auto py-4 space-y-4">
        {#if activeTab === 'local'}
          <div class="space-y-2">
            {#each statuses as client (client.key)}
              <div
                class="flex items-center justify-between rounded-xl border border-zinc-800/80 bg-zinc-900/60 p-3.5 hover:border-zinc-700/80 transition"
              >
                <div class="flex items-center gap-3 min-w-0">
                  <div
                    class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-zinc-800 text-zinc-300"
                  >
                    <Terminal class="h-4 w-4" />
                  </div>
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-xs font-semibold text-zinc-200">{client.name}</span>
                      {#if client.is_configured}
                        <span
                          class="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-400 border border-emerald-500/20"
                        >
                          <Check class="h-2.5 w-2.5" /> Connected
                        </span>
                      {:else}
                        <span
                          class="rounded-full bg-zinc-800 px-2 py-0.5 text-[10px] text-zinc-400"
                        >
                          Not Configured
                        </span>
                      {/if}
                    </div>
                    {#if client.config_path}
                      <p
                        class="truncate text-[11px] text-zinc-500 font-mono"
                        title={client.config_path}
                      >
                        {client.config_path}
                      </p>
                    {/if}
                  </div>
                </div>

                <div class="flex items-center gap-2 shrink-0">
                  {#if client.is_configured}
                    <button
                      onclick={() => handleUninstall(client.key)}
                      disabled={loading}
                      title="Remove Fossil from config"
                      class="cursor-pointer inline-flex items-center gap-1 rounded-lg border border-zinc-800 bg-zinc-800/80 px-2.5 py-1 text-xs text-zinc-400 hover:border-rose-900/50 hover:bg-rose-950/40 hover:text-rose-300 transition"
                    >
                      <Trash2 class="h-3 w-3" />
                      Remove
                    </button>
                  {:else}
                    <button
                      onclick={() => handleInstall(client.key)}
                      disabled={loading}
                      class="cursor-pointer inline-flex items-center gap-1 rounded-lg bg-purple-600 px-3 py-1 text-xs font-medium text-white hover:bg-purple-500 transition shadow-xs"
                    >
                      Connect
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {:else if activeTab === 'chatgpt'}
          <div class="space-y-3 text-xs text-zinc-300">
            <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 space-y-2">
              <h3 class="font-semibold text-zinc-100 flex items-center gap-2">
                <span
                  class="flex h-5 w-5 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-400 text-[10px]"
                  >1</span
                >
                Start Streamable HTTP Server
              </h3>
              <p class="text-zinc-400 text-[11px]">
                Run Fossil's authenticated Streamable HTTP MCP server in your terminal:
              </p>
              <pre
                class="rounded-lg bg-zinc-950 p-2.5 font-mono text-[11px] text-emerald-400 border border-zinc-800/80">fossil mcp serve --http --port 7742</pre>
            </div>

            <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 space-y-2">
              <h3 class="font-semibold text-zinc-100 flex items-center gap-2">
                <span
                  class="flex h-5 w-5 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-400 text-[10px]"
                  >2</span
                >
                Expose Secure Tunnel (for ChatGPT Web)
              </h3>
              <p class="text-zinc-400 text-[11px]">
                If connecting from ChatGPT in a browser, expose your local port securely via
                Cloudflare Tunnel or Tailscale Funnel:
              </p>
              <pre
                class="rounded-lg bg-zinc-950 p-2.5 font-mono text-[11px] text-cyan-400 border border-zinc-800/80">cloudflared tunnel --url http://127.0.0.1:7742</pre>
            </div>

            <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 space-y-2">
              <h3 class="font-semibold text-zinc-100 flex items-center gap-2">
                <span
                  class="flex h-5 w-5 items-center justify-center rounded-full bg-emerald-500/20 text-emerald-400 text-[10px]"
                  >3</span
                >
                Add Connector in ChatGPT
              </h3>
              <p class="text-zinc-400 text-[11px]">
                In ChatGPT Settings &rarr; Connected apps / MCP Connectors:
              </p>
              <ul class="list-disc pl-5 space-y-1 text-zinc-400 text-[11px]">
                <li>
                  <strong class="text-zinc-200">URL:</strong>
                  <code class="text-zinc-300">https://&lt;your-tunnel-domain&gt;/mcp</code>
                  (or <code class="text-zinc-300">http://127.0.0.1:7742/mcp</code> for local desktop)
                </li>
                <li><strong class="text-zinc-200">Auth:</strong> Bearer Token</li>
                <li>
                  <strong class="text-zinc-200">Token:</strong> Paste the token displayed in your terminal.
                </li>
              </ul>
            </div>
          </div>
        {:else}
          <div class="space-y-3">
            <p class="text-xs text-zinc-400">
              To configure any MCP-compatible tool manually, add this configuration to your client's
              config file:
            </p>
            <div class="relative">
              <pre
                class="rounded-xl bg-zinc-900 p-3.5 font-mono text-xs text-zinc-200 border border-zinc-800 overflow-x-auto">{manualSnippet}</pre>
              <button
                onclick={() => copyToClipboard(manualSnippet)}
                class="cursor-pointer absolute top-2.5 right-2.5 inline-flex items-center gap-1 rounded-md bg-zinc-800 px-2 py-1 text-[11px] text-zinc-300 hover:bg-zinc-700 transition"
              >
                {#if copiedSnippet}
                  <Check class="h-3 w-3 text-emerald-400" />
                  <span class="text-emerald-400">Copied</span>
                {:else}
                  <Copy class="h-3 w-3" />
                  <span>Copy</span>
                {/if}
              </button>
            </div>
          </div>
        {/if}

        {#if actionMessage}
          <div
            class="rounded-lg bg-zinc-900/80 border border-zinc-700/80 p-3 text-xs text-zinc-300 font-mono whitespace-pre-wrap"
          >
            {actionMessage}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div
        class="flex items-center justify-between pt-3 border-t border-zinc-800 text-xs text-zinc-500"
      >
        <div class="flex items-center gap-2">
          <span>CLI command:</span>
          <code class="rounded bg-zinc-900 px-1.5 py-0.5 text-zinc-300 font-mono"
            >fossil mcp install &lt;client&gt;</code
          >
        </div>
        <button
          onclick={onclose}
          class="cursor-pointer rounded-lg bg-zinc-800 px-4 py-1.5 text-xs font-medium text-zinc-200 hover:bg-zinc-700 transition"
        >
          Close
        </button>
      </div>
    </div>
  </div>
{/if}
