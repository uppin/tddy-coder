import { VmsScreen, VmRow } from "@/components/vms/VmsScreen";

const mockRows: VmRow[] = [
  {
    name: "web",
    state: "Running",
    sshHostPort: 2222,
    shareUrl: "http://localhost:8080",
    errorMessage: "",
  },
  {
    name: "db",
    state: "Stopped",
    sshHostPort: 0,
    shareUrl: "",
    errorMessage: "",
  },
];

describe("VmsScreen", () => {
  it("shows vms-empty when rows is empty", () => {
    cy.mount(
      <VmsScreen
        rows={[]}
        onStart={cy.stub()}
        onStop={cy.stub()}
        onRemove={cy.stub()}
      />
    );
    cy.get('[data-testid="vms-empty"]').should("be.visible");
    cy.get('[data-testid="vms-table"]').should("not.exist");
  });

  it("shows vms-table when rows are present", () => {
    cy.mount(
      <VmsScreen
        rows={mockRows}
        onStart={cy.stub()}
        onStop={cy.stub()}
        onRemove={cy.stub()}
      />
    );
    cy.get('[data-testid="vms-table"]').should("be.visible");
    cy.get('[data-testid="vms-empty"]').should("not.exist");
  });

  it("renders a row per VM with correct data-testids", () => {
    cy.mount(
      <VmsScreen
        rows={mockRows}
        onStart={cy.stub()}
        onStop={cy.stub()}
        onRemove={cy.stub()}
      />
    );
    cy.get('[data-testid="vms-row-web"]').should("exist");
    cy.get('[data-testid="vms-row-db"]').should("exist");
    cy.get('[data-testid="vms-state-web"]').should("contain.text", "Running");
    cy.get('[data-testid="vms-state-db"]').should("contain.text", "Stopped");
  });

  it("calls onStart when Start button is clicked", () => {
    const onStart = cy.stub().as("onStart");
    cy.mount(
      <VmsScreen
        rows={mockRows}
        onStart={onStart}
        onStop={cy.stub()}
        onRemove={cy.stub()}
      />
    );
    cy.get('[data-testid="vms-start-web"]').click();
    cy.get("@onStart").should("have.been.calledWith", "web");
  });

  it("calls onStop when Stop button is clicked", () => {
    const onStop = cy.stub().as("onStop");
    cy.mount(
      <VmsScreen
        rows={mockRows}
        onStart={cy.stub()}
        onStop={onStop}
        onRemove={cy.stub()}
      />
    );
    cy.get('[data-testid="vms-stop-web"]').click();
    cy.get("@onStop").should("have.been.calledWith", "web");
  });

  it("calls onRemove when Remove button is clicked", () => {
    const onRemove = cy.stub().as("onRemove");
    cy.mount(
      <VmsScreen
        rows={mockRows}
        onStart={cy.stub()}
        onStop={cy.stub()}
        onRemove={onRemove}
      />
    );
    cy.get('[data-testid="vms-remove-db"]').click();
    cy.get("@onRemove").should("have.been.calledWith", "db");
  });
});
