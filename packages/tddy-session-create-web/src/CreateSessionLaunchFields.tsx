import React from "react";
import { CreateSessionToolFields, type CreateSessionToolFieldsProps } from "./CreateSessionToolFields";
import { CreateSessionCliFields, type CreateSessionCliFieldsProps } from "./CreateSessionCliFields";
import { TDDY_CODER_LAUNCH } from "./cliHarnessOptions";

/**
 * The launch-shaped field cluster of the New-session pane (`#orchestrators` 12/12, B12): the
 * tool-session fields for a `tddy-coder` launch, and a CLI agent's spec-shaped fields. Split out
 * of `CreateSessionPane` so the pane holds the form's state and wiring, not every field block.
 */
export function CreateSessionLaunchFields(props: {
  sessionLaunch: string | undefined;
  tool: CreateSessionToolFieldsProps;
  cli: CreateSessionCliFieldsProps | null;
}): React.ReactElement {
  return (
    <>
      {/* Tool session fields */}
      {props.sessionLaunch === TDDY_CODER_LAUNCH && <CreateSessionToolFields {...props.tool} />}

      {/* A CLI agent's fields, shaped by what its spec declares: the permission controls
          when its launch binds a permission mode, the managed block when it can run one. */}
      {props.cli && <CreateSessionCliFields {...props.cli} />}
    </>
  );
}
