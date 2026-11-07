/** One entry of the Start-session dialog's "Base branch" `<select>`: the ref it submits, and its caption. */
export interface BaseBranchOption {
  /** The base ref this option submits as `StartSession.selected_integration_base_ref`. */
  value: string;
  /** What the operator reads. Equal to `value` except for the project default of a legacy project. */
  label: string;
}

/** The whole state of the "Base branch" `<select>`: what it offers, and what it starts on. */
export interface BaseBranchChoice {
  options: BaseBranchOption[];
  selected: string;
}
