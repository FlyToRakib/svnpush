import { describe, expect, it } from "vitest";
import { plural } from "./format";
import { S } from "./strings";

describe("plural", () => {
  it("uses the singular noun for exactly one", () => {
    expect(plural(1, "file", "files")).toBe("1 file");
    expect(plural(0, "file", "files")).toBe("0 files");
    expect(plural(2, "file", "files")).toBe("2 files");
  });

  it("leaves no (s) in counted copy", () => {
    expect(S.help.account.ready(1)).toBe("1 account in Vault.");
    expect(S.help.ai.ready(2)).toBe("2 providers set up.");
    expect(S.providers.requests(1)).toBe("1 request this month");
    expect(S.detect.dirty(1)).toBe("1 uncommitted change");
    expect(S.providers.fleetOnline(1, 1, 0, 1, 1)).toBe(
      "1 of 1 device online · 0 of 1 agent free · 1 job queued",
    );
  });
});
