import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppIdentifier,
  AppMetadata,
  AppIntent,
  Channel,
  Context,
  ContextHandler,
  ImplementationMetadata,
  IntentHandler,
  IntentResolution,
  Listener,
  RaiseIntentResult,
} from "./types";

// ── Internal event shapes from the Rust broker ───────────────────────────────

interface ContextEvent {
  channelId: string;
  context: Context;
  sourceAppId: string;
  sourceInstanceId: string;
}

interface IntentEvent {
  intent: string;
  context: Context;
  sourceAppId: string;
  sourceInstanceId: string;
  requestId: string;
}

// ── Fdc3Agent — implements the FDC3 2.2 DesktopAgent surface ─────────────────

export class Fdc3Agent {
  private instanceId: string;
  private appId: string;

  private constructor(appId: string, instanceId: string) {
    this.appId = appId;
    this.instanceId = instanceId;
  }

  /** Factory — registers with the broker and returns a ready agent. */
  static async create(appId: string, windowLabel: string): Promise<Fdc3Agent> {
    const identity = await invoke<AppIdentifier>("fdc3_register", {
      appId,
      windowLabel,
    });
    return new Fdc3Agent(appId, identity.instanceId!);
  }

  getIdentity(): AppIdentifier {
    return { appId: this.appId, instanceId: this.instanceId };
  }

  async getInfo(): Promise<ImplementationMetadata> {
    return {
      fdc3Version: "2.2",
      provider: "one-terminal",
      providerVersion: "0.2.0",
      appMetadata: { appId: this.appId, instanceId: this.instanceId },
    };
  }

  // ── Registration ────────────────────────────────────────────────────────────

  async getRegisteredApps(): Promise<AppMetadata[]> {
    return invoke<AppMetadata[]>("fdc3_get_registered_apps");
  }

  // ── Channels ─────────────────────────────────────────────────────────────────

  async getSystemChannels(): Promise<Channel[]> {
    return invoke<Channel[]>("fdc3_get_system_channels");
  }

  /** Alias for getSystemChannels (FDC3 2.2 renamed to getUserChannels). */
  async getUserChannels(): Promise<Channel[]> {
    return this.getSystemChannels();
  }

  async getOrCreateChannel(channelId: string): Promise<Channel> {
    return invoke<Channel>("fdc3_get_or_create_channel", { id: channelId });
  }

  async createPrivateChannel(): Promise<Channel> {
    return invoke<Channel>("fdc3_create_private_channel");
  }

  async joinUserChannel(channelId: string): Promise<void> {
    return invoke("fdc3_join_user_channel", {
      instanceId: this.instanceId,
      channelId,
    });
  }

  async leaveCurrentChannel(): Promise<void> {
    return invoke("fdc3_leave_current_channel", { instanceId: this.instanceId });
  }

  async getCurrentChannel(): Promise<Channel | null> {
    return invoke<Channel | null>("fdc3_get_current_channel", {
      instanceId: this.instanceId,
    });
  }

  async getCurrentContext(contextType?: string): Promise<Context | null> {
    const ch = await this.getCurrentChannel();
    if (!ch) return null;
    return invoke<Context | null>("fdc3_get_current_context", {
      channelId: ch.id,
      contextType: contextType ?? null,
    });
  }

  // ── Context ──────────────────────────────────────────────────────────────────

  /** Broadcasts context on the caller's current channel. */
  async broadcast(context: Context): Promise<void> {
    console.warn("fdc3_broadcast", {
      instanceId: this.instanceId,
      context,
    });
    return invoke("fdc3_broadcast", {
      instanceId: this.instanceId,
      context,
    });
  }

  /**
   * Subscribe to context events on the current channel.
   *
   * - `contextType = null`  → receive all context types
   * - `contextType = "fdc3.instrument"` → receive only that type
   *
   * Returns a `Listener` whose `unsubscribe()` cleans up the event handler.
   */
  addContextListener(contextType: string | null, handler: ContextHandler): Listener {
    const unlistenPromise = listen<ContextEvent>("fdc3:context", (event) => {
      const { context, sourceAppId, sourceInstanceId } = event.payload;
      if (contextType === null || context.type === contextType) {
        handler(context, {
          source: { appId: sourceAppId, instanceId: sourceInstanceId },
        });
      }
    });

    return {
      unsubscribe() {
        unlistenPromise.then((fn) => fn());
      },
    };
  }

  // ── Intents ───────────────────────────────────────────────────────────────────

  /**
   * Register a handler for an intent.
   * Notifies the broker AND wires up the `fdc3:intent` event listener.
   */
  addIntentListener(intent: string, handler: IntentHandler): Listener {
    invoke("fdc3_add_intent_listener", {
      instanceId: this.instanceId,
      intent,
    });

    const unlistenPromise = listen<IntentEvent>("fdc3:intent", (event) => {
      if (event.payload.intent === intent) {
        const { context, sourceAppId, sourceInstanceId, requestId } = event.payload;
        handler(context, {
          source: { appId: sourceAppId, instanceId: sourceInstanceId },
          requestId,
        });
      }
    });

    const instanceId = this.instanceId;
    return {
      unsubscribe() {
        invoke("fdc3_remove_intent_listener", { instanceId, intent });
        unlistenPromise.then((fn) => fn());
      },
    };
  }

  async findIntent(intent: string, contextType?: string): Promise<AppIntent> {
    return invoke<AppIntent>("fdc3_find_intent", {
      intent,
      contextType: contextType ?? null,
    });
  }

  async findIntentsForContext(contextType: string): Promise<AppIntent[]> {
    return invoke<AppIntent[]>("fdc3_find_intents_for_context", { contextType });
  }

  async raiseIntent(
    intent: string,
    context: Context,
    target?: AppIdentifier
  ): Promise<IntentResolution> {
    const result = await invoke<RaiseIntentResult>("fdc3_raise_intent", {
      sourceInstanceId: this.instanceId,
      intent,
      context,
      target: target ?? null,
    });

    if (result.kind === "resolved") return result.data;

    // Multiple handlers — caller must pick a target and call again
    throw Object.assign(new Error("ResolverRequired"), {
      candidates: result.data,
    });
  }

  async raiseIntentForContext(context: Context, target?: AppIdentifier): Promise<IntentResolution> {
    const result = await invoke<RaiseIntentResult>("fdc3_raise_intent_for_context", {
      sourceInstanceId: this.instanceId,
      context,
      target: target ?? null,
    });

    if (result.kind === "resolved") return result.data;

    throw Object.assign(new Error("ResolverRequired"), {
      candidates: result.data,
    });
  }
}
