import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { highlightText } from "./highlight";

function renderNode(node: React.ReactNode) {
  return render(<div data-testid="root">{node}</div>);
}

describe("highlightText", () => {
  it("returns raw text when query is empty", () => {
    const result = highlightText("hello world", "");
    expect(result).toBe("hello world");
  });

  it("returns raw text when no match", () => {
    const result = highlightText("hello", "xyz");
    expect(result).toBe("hello");
  });

  it("wraps a single case-insensitive match in .hl span", () => {
    const { container } = renderNode(highlightText("Alice", "ali"));
    const hl = container.querySelector(".hl");
    expect(hl).not.toBeNull();
    expect(hl?.textContent).toBe("Ali");
  });

  it("wraps multiple matches", () => {
    const { container } = renderNode(highlightText("banana", "an"));
    const matches = container.querySelectorAll(".hl");
    expect(matches.length).toBe(2);
    expect(matches[0].textContent).toBe("an");
    expect(matches[1].textContent).toBe("an");
  });

  it("preserves surrounding text around a match", () => {
    const { getByTestId } = renderNode(highlightText("Hello World", "llo"));
    expect(getByTestId("root").textContent).toBe("Hello World");
  });
});
