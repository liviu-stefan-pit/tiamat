import { render, screen } from "@testing-library/react";
import App from "./App";

describe("App shell", () => {
  it("renders the Tiamat shell layout", () => {
    render(<App />);
    expect(screen.getByTestId("tiamat-shell")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Tiamat" })).toBeInTheDocument();
    expect(screen.getByLabelText("Intake placeholder")).toBeInTheDocument();
    expect(screen.getByLabelText("Graph placeholder")).toBeInTheDocument();
    expect(screen.getByLabelText("Activity log placeholder")).toBeInTheDocument();
  });
});
