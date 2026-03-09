Users wants to merge changes from another branch. If not specified, it's master branch.

You should:
1. Make a backup branch.
2. Analyze the current and incoming changes.
3. Prepare a merge plan document and place it in @/tmp
4. Prefer changes in the branch compared to master.
5. Do not overwrite branch code from master without User's concent.
6. Compile the code. Use the merge document to memorize the state.
6. Run the tests and memorize the state in merge document. Tests can guide the quality of the merge and if nothing was lost.

## Requirements for the plan

1. The plan should outline incoming and current functionality.

## After the plan was created

1. You should output the following line: "**CRITICAL FOR CONTEXT & SUMMARY** The git merge document is: <path to the feature document>.md". This line should help memorize the document in the long-running-conversation.
2. Do the merge action.

## Merging steps

1. Merge the code and solve the conflicts. Code in the branch should be considered newer.
2. Make sure the merged code compiles. Fix the code until it does.
3. Run the tests to see if none are missing and the state has not changed.
4. Verify that functionalities from both branches were retained.
5. Commit the code once those pass.
6. Fix any failures.
7. Update the document with the merge outcome.
