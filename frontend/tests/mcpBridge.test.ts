import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { WINDOW_ID } from "../src/lib/api";
import { PROJECT_CHANGED, SETTINGS_CHANGED, maskSecret, readPage, showChosenOptions, startBridge, takeArgs } from "../src/lib/mcpBridge";

/** The stream the studio opens to the page, driven by the test. */
class FakeStream {
  static last: FakeStream | null = null;
  onmessage: ((message: { data: string }) => void | Promise<void>) | null = null;
  onerror: (() => void) | null = null;
  url: string;
  constructor(url: string) {
    this.url = url;
    FakeStream.last = this;
  }
  close() {}
  async send(data: unknown) {
    await this.onmessage?.({ data: JSON.stringify(data) });
  }
}

const posted: Array<{ url: string; body: Record<string, unknown> }> = [];

beforeAll(() => {
  // happy-dom lays nothing out: every element has a size, so the page reads as visible
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, width: 10, height: 10, top: 0, left: 0, right: 10, bottom: 10, toJSON: () => ({}) });
  vi.stubGlobal("EventSource", FakeStream);
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    posted.push({ url, body: JSON.parse(String(init?.body ?? "null")) });
    return new Response(null, { status: 204 });
  }));
  startBridge();
});

afterAll(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the page's side of the bridge", () => {
  it("answers a command for its window by the command's id, and ignores another window's", async () => {
    const stream = FakeStream.last!;
    expect(stream.url.endsWith("/mcp/window")).toBe(true);
    await stream.send({ window: 7 });
    document.body.innerHTML = `<button title="Export"><svg></svg></button>`;
    posted.length = 0;
    await stream.send({ id: "w1", window: 8, command: "read_page", args: {} });
    expect(posted.filter((p) => p.url.endsWith("/mcp/window/result"))).toEqual([]);
    await stream.send({ id: "w2", window: 7, command: "read_page", args: {} });
    const answer = posted.find((p) => p.url.endsWith("/mcp/window/result"))!;
    expect(answer.body.id).toBe("w2");
    expect(String((answer.body.result as { text: string }).text)).toContain('button "Export"');
  });

  it("draws a list of the page's copy at the option chosen, not its first", () => {
    const copy = document.createElement("div");
    copy.innerHTML = `<select value="ru"><option value="en" selected>EN</option><option value="ru">RU</option></select>`;
    showChosenOptions(copy);
    const options = Array.from(copy.querySelectorAll("option"));
    expect(options.map((option) => option.hasAttribute("selected"))).toEqual([false, true]);
  });

  it("reads a button's title and caption as separate words", () => {
    document.body.innerHTML = `<button><span>Models</span><span>Local engines and quants</span></button>`;
    expect(readPage()).toContain('button "Models Local engines and quants"');
  });

  it("clicks by label inside an open dialog, not the control of the same name behind it", async () => {
    const stream = FakeStream.last!;
    const clicked: string[] = [];
    document.body.innerHTML = `<button id="behind" title="Delete">x</button>
      <div role="alertdialog" aria-modal="true"><button id="cancel">Cancel</button><button id="confirm">Delete</button></div>`;
    for (const id of ["behind", "confirm"]) document.getElementById(id)!.addEventListener("click", () => clicked.push(id));
    posted.length = 0;
    await stream.send({ id: "w9", window: 7, command: "click", args: { text: "Delete" } });
    expect(clicked).toEqual(["confirm"]);
    expect(posted.at(-1)!.body.error).toBeUndefined();
  });

  it("says which command it cannot do on this screen, and refuses a field a command does not take", async () => {
    const stream = FakeStream.last!;
    posted.length = 0;
    await stream.send({ id: "w3", window: 7, command: "editor_segment_split", args: {} });
    expect(String(posted.at(-1)!.body.error)).toContain("cannot do \"editor_segment_split\"");
    await stream.send({ id: "w4", window: 7, command: "read_page", args: { verbose: true } });
    expect(posted.at(-1)!.body).toEqual({ id: "w4", error: "ui_read_page does not take verbose; it takes nothing." });
  });

  it("clicks a control by its label and reports it", async () => {
    const stream = FakeStream.last!;
    const clicked = vi.fn();
    document.body.innerHTML = `<button aria-label="Delete project">x</button>`;
    document.querySelector("button")!.addEventListener("click", clicked);
    posted.length = 0;
    await stream.send({ id: "w5", window: 7, command: "click", args: { text: "delete project" } });
    expect(clicked).toHaveBeenCalledOnce();
    expect(posted.at(-1)!.body).toEqual({ id: "w5", result: { text: "Clicked Delete project." } });
  });

  it("passes a change on to the screens, but not the window's own list changes", async () => {
    const stream = FakeStream.last!;
    const projects: unknown[] = [];
    const settings: unknown[] = [];
    window.addEventListener(PROJECT_CHANGED, (e) => projects.push((e as CustomEvent).detail));
    window.addEventListener(SETTINGS_CHANGED, (e) => settings.push((e as CustomEvent).detail));
    await stream.send({ changed: "project", pid: "p1", rev: 3, by: "agent" });
    await stream.send({ changed: "settings", by: `window:${WINDOW_ID}` });
    await stream.send({ changed: "settings", by: "agent" });
    expect(projects).toEqual([{ changed: "project", pid: "p1", rev: 3, by: "agent" }]);
    expect(settings).toEqual([{ changed: "settings", by: "agent" }]);
  });
});

describe("what the agent reads of the page", () => {
  it("lists an open dialog first and a list's row as one line with its fields' values", () => {
    document.body.innerHTML = `
      <div data-mcp-context="segment s1 0:01.0→0:02.0 SPK 1: Hi"><button title="Play"><svg></svg></button><textarea>Привет</textarea></div>
      <div role="dialog" aria-modal="true"><button>Cancel</button><button>Delete project</button></div>`;
    const page = readPage();
    const lines = page.split("\n");
    expect(lines[0]).toContain("A dialog is open");
    expect(lines[1]).toMatch(/button "Cancel"$/);
    const row = lines.find((line) => line.startsWith("segment s1"))!;
    expect(row).toMatch(/: e\d+ open, e\d+ Play, e\d+ textarea="Привет"$/);
  });

  it("names a button by its words before its tooltip, and an icon button by its tooltip", () => {
    document.body.innerHTML = `<button title="Remove the line">Edit</button><button title="Play"><svg></svg></button>`;
    const page = readPage();
    expect(page).toMatch(/button "Edit"/);
    expect(page).toMatch(/button "Play"/);
  });

  it("keeps a closed section's content out and reads the section with its state", () => {
    document.body.innerHTML = `<details><summary>Advanced</summary><button>Hidden inside</button></details>`;
    const page = readPage();
    expect(page).toMatch(/section "Advanced" closed/);
    expect(page).not.toMatch(/Hidden inside/);
  });

  it("never reads out a key or a password the user typed, even one its show button reveals", () => {
    document.body.innerHTML = `
      <input type="password" aria-label="OpenRouter key">
      <input type="text" data-mcp-secret aria-label="Proxy password">
      <input type="password" aria-label="Empty key" placeholder="sk-or-...">
      <input type="text" aria-label="Proxy address">
      <div data-mcp-context="row r1"><input type="password" aria-label="Row key"></div>`;
    const [key, shown, , address] = Array.from(document.querySelectorAll("input"));
    key.value = "sk-or-v1-secret-key";
    shown.value = "proxy-pass-123";
    address.value = "http://127.0.0.1:8080";
    document.querySelector<HTMLInputElement>("[data-mcp-context] input")!.value = "sk-or-row-secret";
    const page = readPage();
    for (const typed of ["sk-or-v1-secret-key", "proxy-pass-123", "sk-or-row-secret"]) expect(page).not.toContain(typed);
    expect(page).toMatch(/input password "OpenRouter key" filled/);
    expect(page).toMatch(/input text "Proxy password" filled/);
    expect(page).toMatch(/input password "Empty key" empty placeholder="sk-or-..."/);
    expect(page).toContain('value="http://127.0.0.1:8080"');
    expect(page).toMatch(/row r1: e\d+ open, e\d+ Row key=filled$/m);
  });

  it("masks a revealed secret field in the window's picture", () => {
    document.body.innerHTML = `<input type="text" data-mcp-secret value="proxy-pass-123"><input type="text" value="visible">`;
    const [secretField, plain] = Array.from(document.querySelectorAll("input")).map((input) => input.cloneNode() as HTMLInputElement);
    maskSecret(secretField);
    maskSecret(plain);
    expect([secretField.type, plain.type]).toEqual(["password", "text"]);
  });

  it("refuses unknown arguments by name", () => {
    expect(() => takeArgs("editor_seek", { secs: 3 }, ["seconds", "segment_id"])).toThrow("editor_seek does not take secs; it takes seconds, segment_id.");
    expect(takeArgs("editor_seek", { seconds: 3 }, ["seconds"])).toEqual({ seconds: 3 });
  });
});
