import { DefineVmPanel } from "@/components/vms/DefineVmPanel";

const noop = () => {};
const idle = {
  building: false,
  availableImages: [] as string[],
  errorMessage: "",
  buildLog: [] as string[],
  onBuild: noop,
  onDefineVm: noop,
};

const TWO_IMAGES = [
  "/var/tddy/images/minimal.qcow2",
  "/var/tddy/images/full.qcow2",
];

const SAMPLE_SPEC = `BR2_x86_64=y
BR2_TOOLCHAIN_BUILDROOT_GLIBC=y
BR2_TARGET_ROOTFS_EXT2=y`;

// ── Build spec section ────────────────────────────────────────────────────────

describe("DefineVmPanel — build spec", () => {
  it("renders a textarea for the buildroot spec", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-spec"]').should("be.visible");
  });

  it("textarea is pre-filled with a minimal busybox x86_64 spec", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-spec"]')
      .should("contain.value", "BR2_x86_64=y")
      .and("contain.value", "BR2_PACKAGE_BUSYBOX=y")
      .and("contain.value", "BR2_TARGET_ROOTFS_EXT2=y");
  });

  it("Build button is disabled when the spec textarea is empty", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-spec"]').clear();
    cy.get('[data-testid="define-vm-build-btn"]').should("be.disabled");
  });

  it("Build button is enabled when the spec textarea has content", () => {
    cy.mount(<DefineVmPanel {...idle} />);
    cy.get('[data-testid="define-vm-build-btn"]').should("not.be.disabled");
  });

  it("calls onBuild with the textarea content when Build is clicked", () => {
    const onBuild = cy.stub().as("onBuild");
    cy.mount(<DefineVmPanel {...idle} onBuild={onBuild} />);
    cy.get('[data-testid="define-vm-spec"]').type(SAMPLE_SPEC);
    cy.get('[data-testid="define-vm-build-btn"]').click();
    cy.get("@onBuild").should("have.been.calledOnce");
    cy.get("@onBuild").its("firstCall.args.0").should("include", "BR2_x86_64=y");
  });

  it("Build button is disabled while building=true", () => {
    cy.mount(<DefineVmPanel {...idle} building={true} />);
    cy.get('[data-testid="define-vm-build-btn"]').should("be.disabled");
  });

  it("shows connecting indicator when building and buildLog is empty", () => {
    cy.mount(<DefineVmPanel {...idle} building={true} buildLog={[]} />);
    cy.get('[data-testid="define-vm-connecting"]').should("be.visible");
  });

  it("connecting indicator is absent when not building", () => {
    cy.mount(<DefineVmPanel {...idle} building={false} buildLog={[]} />);
    cy.get('[data-testid="define-vm-connecting"]').should("not.exist");
  });

  it("connecting indicator is absent once buildLog has entries", () => {
    cy.mount(<DefineVmPanel {...idle} building={true} buildLog={["Starting..."]} />);
    cy.get('[data-testid="define-vm-connecting"]').should("not.exist");
  });

  it("build log container is absent when buildLog is empty", () => {
    cy.mount(<DefineVmPanel {...idle} buildLog={[]} />);
    cy.get('[data-testid="define-vm-build-log"]').should("not.exist");
  });

  it("shows build log container when buildLog has entries", () => {
    const log = ["Configuring...", "Building rootfs...", "Converting to qcow2..."];
    cy.mount(<DefineVmPanel {...idle} buildLog={log} />);
    cy.get('[data-testid="define-vm-build-log"]').should("be.visible");
  });

  it("renders one entry per buildLog item in order", () => {
    const log = ["Step 1", "Step 2", "Step 3"];
    cy.mount(<DefineVmPanel {...idle} buildLog={log} />);
    cy.get('[data-testid="define-vm-build-log-entry"]').should("have.length", 3);
    cy.get('[data-testid="define-vm-build-log-entry"]').first().should("contain.text", "Step 1");
    cy.get('[data-testid="define-vm-build-log-entry"]').last().should("contain.text", "Step 3");
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
});

// ── Create VM section ─────────────────────────────────────────────────────────

describe("DefineVmPanel — create VM", () => {
  it("renders a dropdown for image selection", () => {
    cy.mount(<DefineVmPanel {...idle} availableImages={TWO_IMAGES} />);
    cy.get('[data-testid="define-vm-image-select"]').should("be.visible");
  });

  it("dropdown shows a placeholder option when no images are available", () => {
    cy.mount(<DefineVmPanel {...idle} availableImages={[]} />);
    cy.get('[data-testid="define-vm-image-select"]')
      .find("option")
      .first()
      .should("have.attr", "disabled");
  });

  it("dropdown lists every available image as a selectable option", () => {
    cy.mount(<DefineVmPanel {...idle} availableImages={TWO_IMAGES} />);
    cy.get('[data-testid="define-vm-image-select"] option').should(
      "have.length.at.least",
      TWO_IMAGES.length
    );
    cy.get('[data-testid="define-vm-image-select"]').contains(
      "minimal.qcow2"
    );
    cy.get('[data-testid="define-vm-image-select"]').contains("full.qcow2");
  });

  it("Create button is disabled when availableImages is empty", () => {
    cy.mount(<DefineVmPanel {...idle} availableImages={[]} />);
    cy.get('[data-testid="define-vm-name"]').type("web-vm");
    cy.get('[data-testid="define-vm-create-btn"]').should("be.disabled");
  });

  it("Create button is disabled when VM name is empty", () => {
    cy.mount(<DefineVmPanel {...idle} availableImages={TWO_IMAGES} />);
    cy.get('[data-testid="define-vm-image-select"]').select(TWO_IMAGES[0]);
    cy.get('[data-testid="define-vm-create-btn"]').should("be.disabled");
  });

  it("calls onDefineVm with vm name and the selected image path when Create is clicked", () => {
    const onDefineVm = cy.stub().as("onDefineVm");
    cy.mount(
      <DefineVmPanel
        {...idle}
        availableImages={TWO_IMAGES}
        onDefineVm={onDefineVm}
      />
    );
    cy.get('[data-testid="define-vm-image-select"]').select(TWO_IMAGES[1]);
    cy.get('[data-testid="define-vm-name"]').type("my-vm");
    cy.get('[data-testid="define-vm-create-btn"]').click();
    cy.get("@onDefineVm").should(
      "have.been.calledWith",
      "my-vm",
      TWO_IMAGES[1]
    );
  });
});
