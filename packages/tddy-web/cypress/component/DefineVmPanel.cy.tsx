import { DefineVmPanel } from "@/components/vms/DefineVmPanel";

// Shared idle props — use plain no-ops where no assertion is needed.
// Tests that assert a callback override the relevant prop with cy.stub().
const noop = () => {};
const idle = {
  building: false,
  builtImagePath: "",
  errorMessage: "",
  onBuildImage: noop,
  onDefineVm: noop,
};

describe("DefineVmPanel", () => {
  it("renders build-target input and Build button", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-build-target"]').should("be.visible");
    cy.get('[data-testid="define-vm-build-btn"]').should("be.visible");
  });

  it("Build button is disabled when build-target input is empty", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-build-btn"]').should("be.disabled");
  });

  it("Build button is enabled after typing a build target", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-build-target"]').type("qemu-minimal:qcow2");
    cy.get('[data-testid="define-vm-build-btn"]').should("not.be.disabled");
  });

  it("calls onBuildImage with the typed build target when Build is clicked", () => {
    const onBuildImage = cy.stub().as("onBuildImage");
    cy.mount(<DefineVmPanel {...idle} onBuildImage={onBuildImage} />);
    cy.get('[data-testid="define-vm-build-target"]').type("qemu-minimal:qcow2");
    cy.get('[data-testid="define-vm-build-btn"]').click();
    cy.get("@onBuildImage").should("have.been.calledWith", "qemu-minimal:qcow2");
  });

  it("shows a building indicator and disables Build while building prop is true", () => {
    cy.mount(<DefineVmPanel {...idle} building={true} />);
    cy.get('[data-testid="define-vm-building-status"]').should("be.visible");
    cy.get('[data-testid="define-vm-build-btn"]').should("be.disabled");
  });

  it("shows the built image path when builtImagePath is provided", () => {
    cy.mount(
      <DefineVmPanel
        {...idle}
        builtImagePath="/var/tddy/build/images/rootfs.qcow2"
      />
    );
    cy.get('[data-testid="define-vm-image-path"]').should(
      "contain.text",
      "/var/tddy/build/images/rootfs.qcow2"
    );
  });

  it("shows an error message when errorMessage is provided", () => {
    cy.mount(
      <DefineVmPanel {...idle} errorMessage="build failed: missing config" />
    );
    cy.get('[data-testid="define-vm-error"]').should(
      "contain.text",
      "build failed: missing config"
    );
  });

  it("Create button is disabled when VM name is empty", () => {
    cy.mount(
      <DefineVmPanel {...idle} builtImagePath="/images/rootfs.qcow2" />
    );
    cy.get('[data-testid="define-vm-create-btn"]').should("be.disabled");
  });

  it("Create button is disabled when no image path is set", () => {
    cy.mount(<DefineVmPanel {...idle} builtImagePath="" />);
    cy.get('[data-testid="define-vm-name"]').type("web-vm");
    cy.get('[data-testid="define-vm-create-btn"]').should("be.disabled");
  });

  it("calls onDefineVm with vm name and built image path when Create is clicked", () => {
    const onDefineVm = cy.stub().as("onDefineVm");
    cy.mount(
      <DefineVmPanel
        {...idle}
        builtImagePath="/images/rootfs.qcow2"
        onDefineVm={onDefineVm}
      />
    );
    cy.get('[data-testid="define-vm-name"]').type("web-vm");
    cy.get('[data-testid="define-vm-create-btn"]').click();
    cy.get("@onDefineVm").should(
      "have.been.calledWith",
      "web-vm",
      "/images/rootfs.qcow2"
    );
  });
});
