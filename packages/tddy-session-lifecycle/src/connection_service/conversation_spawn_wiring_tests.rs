use super::recipe_enables_conversation_spawn;

/// Only the grill-me recipe binds a conversation-spawn handler on its managed session; other
/// recipes (a plain TDD session, or the PR-stack orchestrator which uses `spawn-child` instead)
/// must not, so `spawn_conversation` is rejected there rather than silently spawning.
#[test]
fn grill_me_recipe_enables_conversation_spawn_but_others_do_not() {
    // Then
    assert!(
        recipe_enables_conversation_spawn("grill-me"),
        "grill-me must enable the conversation-spawn handler"
    );
    assert!(
        !recipe_enables_conversation_spawn("tdd"),
        "a plain tdd session must not enable the conversation-spawn handler"
    );
    assert!(
        !recipe_enables_conversation_spawn("pr-stack"),
        "pr-stack uses spawn-child, not spawn-conversation"
    );
}
