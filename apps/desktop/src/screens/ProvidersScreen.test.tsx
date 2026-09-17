import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { AdapterInfo } from "../ipc/bindings/AdapterInfo";
import type { ProviderRecord } from "../ipc/bindings/ProviderRecord";
import { useProviderStore } from "../store/providerStore";
import { tauriMock } from "../test/tauriMock";
import { ProvidersScreen } from "./ProvidersScreen";

const base = {
  recommended: false,
  fixed_base_url: false,
  models: [],
  keyless: false,
  can_list_models: false,
  has_fleet: false,
  note: "",
  model_hint: "",
  key_placeholder: "sk-…",
};

const ADAPTERS: AdapterInfo[] = [
  {
    ...base,
    kind: "revoye",
    label: "Revoye",
    recommended: true,
    default_base_url: "https://api.revoye.com",
    fixed_base_url: true,
    default_model: "revoye/auto",
    models: ["revoye/auto", "chatgpt", "claude"],
    can_list_models: true,
    has_fleet: true,
    note: "Answers come from AI accounts you are already signed into.",
    model_hint: "revoye/auto lets Revoye choose.",
    key_placeholder: "revoye_sk_live_…",
  },
  {
    ...base,
    kind: "openai",
    label: "OpenAI",
    default_base_url: "https://api.openai.com/v1",
    default_model: "gpt-6-astra",
    model_hint: "Default: gpt-6-astra.",
  },
  {
    ...base,
    kind: "openrouter",
    label: "OpenRouter",
    default_base_url: "https://openrouter.ai/api/v1",
    default_model: "anthropic/claude-sonnet-5",
    models: ["anthropic/claude-sonnet-5"],
    can_list_models: true,
  },
  {
    ...base,
    kind: "local",
    label: "Local model (Ollama / LM Studio)",
    default_base_url: "http://localhost:11434/v1",
    default_model: "gemma3",
    keyless: true,
  },
];

function record(overrides: Partial<ProviderRecord>): ProviderRecord {
  return {
    id: "prov_1",
    kind: "revoye",
    label: "Revoye",
    model: "revoye/auto",
    base_url: null,
    has_key: true,
    is_default: true,
    needs_attention: null,
    created_at: "2026-09-17T00:00:00Z",
    requests_this_month: 3,
    usage_month: "2026-09",
    ...overrides,
  };
}

function setup(providers: ProviderRecord[] = [], fallback: string[] = []) {
  useProviderStore.setState({
    adapters: [],
    file: { schema: 1, providers: [], fallback: [] },
    loaded: false,
  });
  tauriMock.handle("provider_adapters", () => ADAPTERS);
  tauriMock.handle("list_providers", () => ({ schema: 1, providers, fallback }));
  return render(<ProvidersScreen />);
}

async function openAdd() {
  const user = userEvent.setup();
  await user.click(
    (await screen.findAllByRole("button", { name: "Add provider" }))[0] as HTMLElement,
  );
  return user;
}

describe("ProvidersScreen", () => {
  beforeEach(() => {
    tauriMock.reset();
  });

  it("shows the empty state with the recommendation", async () => {
    setup();
    expect(
      await screen.findByText(/Revoye is recommended\. A local model works without an API key\./),
    ).toBeTruthy();
  });

  it("lists Revoye first with (Recommended) and hides its base URL", async () => {
    setup();
    const user = await openAdd();
    const picker = screen.getByRole("combobox", { name: "Provider" });
    const options = within(picker)
      .getAllByRole("option")
      .map((o) => o.textContent);
    expect(options[0]).toBe("Revoye (Recommended)");
    expect(
      screen.getByText("Answers come from AI accounts you are already signed into."),
    ).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Model" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Load the models this account can use" }),
    ).toBeTruthy();
    expect(screen.getByLabelText("API key")).toBeTruthy();
    await user.click(screen.getByText("Advanced"));
    expect(screen.queryByLabelText("Base URL")).toBeNull();
  });

  it("uses free text for a closed-list-less adapter and shows its base URL", async () => {
    setup();
    const user = await openAdd();
    await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "openai");
    const model = screen.getByRole<HTMLInputElement>("textbox", { name: "Model" });
    expect(model.value).toBe("gpt-6-astra");
    expect(
      screen.queryByRole("button", { name: "Load the models this account can use" }),
    ).toBeNull();
    expect(screen.getByText("Default: gpt-6-astra.")).toBeTruthy();
    await user.click(screen.getByText("Advanced"));
    expect(screen.getByLabelText("Base URL")).toBeTruthy();
  });

  it("hides the key for a keyless local model and opens Advanced", async () => {
    setup();
    const user = await openAdd();
    await user.selectOptions(screen.getByRole("combobox", { name: "Provider" }), "local");
    expect(screen.queryByLabelText("API key")).toBeNull();
    expect(screen.getByLabelText("Base URL")).toBeTruthy();
    tauriMock.handle("save_provider", () => ({
      schema: 1,
      providers: [record({ kind: "local", has_key: false })],
      fallback: [],
    }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    const input = tauriMock.calls.find((c) => c.command === "save_provider")?.args?.input as Record<
      string,
      unknown
    >;
    expect(input.kind).toBe("local");
    expect(input.api_key).toBeNull();
    expect(input.model).toBe("gemma3");
  });

  it("loads the account's models silently when a key is pasted, with the fleet line", async () => {
    setup();
    const user = await openAdd();
    tauriMock.handle("list_provider_models", () => ({
      models: ["revoye/auto", "chatgpt", "deepseek"],
      note: null,
    }));
    tauriMock.handle("provider_fleet", () => ({
      devices_total: 2,
      devices_online: 1,
      agents_total: 7,
      agents_idle: 3,
      queue_depth: 0,
      providers: [{ kind: "chatgpt", enabled: true, agents_idle: 2 }],
    }));
    await user.type(screen.getByLabelText("API key"), "revoye_sk_live_abc");
    await user.tab();
    expect(await screen.findByText("1 of 2 device(s) online · 3 of 7 agents free")).toBeTruthy();
    expect(screen.getByRole("option", { name: "chatgpt — 2 agent(s) free" })).toBeTruthy();
    const target = tauriMock.calls.find((c) => c.command === "list_provider_models")?.args?.target;
    expect(target).toEqual({
      id: null,
      kind: "revoye",
      base_url: null,
      api_key: "revoye_sk_live_abc",
    });
  });

  it("edits with a write-only key and loads models from the stored key", async () => {
    tauriMock.handle("list_provider_models", () => ({
      models: ["revoye/auto", "claude"],
      note: null,
    }));
    tauriMock.handle("provider_fleet", () => ({
      devices_total: 1,
      devices_online: 0,
      agents_total: 1,
      agents_idle: 0,
      queue_depth: 2,
      providers: [],
    }));
    setup([record({})]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Edit" }));
    const key = screen.getByLabelText<HTMLInputElement>("API key");
    expect(key.placeholder).toBe("Unchanged — paste a new key to replace it");
    expect(key.value).toBe("");
    expect(
      await screen.findByText("No device online, 1 paired. Jobs wait until one connects."),
    ).toBeTruthy();
    expect(tauriMock.calls.find((c) => c.command === "list_provider_models")?.args?.target).toEqual(
      {
        id: "prov_1",
        kind: "revoye",
        base_url: null,
        api_key: null,
      },
    );
  });

  it("shows attention with Clear, tests a provider, and confirms removal", async () => {
    setup([record({ needs_attention: "The key was rejected." })]);
    const user = userEvent.setup();
    expect(await screen.findByText("Needs attention: The key was rejected.")).toBeTruthy();
    expect(screen.getByText("3 request(s) this month")).toBeTruthy();
    tauriMock.handle(
      "test_provider",
      () => "Connected. 1 of 1 device(s) online, 1 of 1 agents free.",
    );
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(
      await screen.findByText("Connected. 1 of 1 device(s) online, 1 of 1 agents free."),
    ).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("dialog", { name: "Remove this provider" });
    expect(tauriMock.calls.some((c) => c.command === "remove_provider")).toBe(false);
    tauriMock.handle("remove_provider", () => ({ schema: 1, providers: [], fallback: [] }));
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(tauriMock.calls.find((c) => c.command === "remove_provider")?.args).toEqual({
      id: "prov_1",
    });
  });

  it("saves an ordered, opt-in fallback chain", async () => {
    const records = [
      record({}),
      record({ id: "prov_2", kind: "local", label: "Local", model: "gemma3", is_default: false }),
    ];
    setup(records);
    const user = userEvent.setup();
    await user.click(await screen.findByText("Automatic fallback"));
    await user.click(screen.getByRole("checkbox", { name: "Local" }));
    await user.click(screen.getByRole("button", { name: "Move up: Local" }));
    await user.click(screen.getByRole("checkbox", { name: "Revoye" }));
    tauriMock.handle("save_provider_fallback", (args) => ({
      schema: 1,
      providers: records,
      fallback: args?.order,
    }));
    await user.click(screen.getByRole("button", { name: "Save fallback order" }));
    expect(tauriMock.calls.find((c) => c.command === "save_provider_fallback")?.args).toEqual({
      order: ["prov_2", "prov_1"],
    });
    expect(await screen.findByText("Fallback saved: 2 provider(s) in order.")).toBeTruthy();
  });
});
