/// One contiguous run of `before` lines and what replaces it. `from` and `to` index `before`.
pub(crate) struct ChangedRegion<'a> {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) lines: Vec<&'a str>,
}

/// The regions in which `before` and `after` differ.
///
/// The lines the two versions share are what make the result useful: they are the coordinates a
/// later anchor can still be translated into, so the diff recognises as many of them as it can
/// rather than settling for a common prefix and suffix.
pub(crate) fn changed_regions<'a>(before: &[&'a str], after: &[&'a str]) -> Vec<ChangedRegion<'a>> {
    let mut regions = Vec::new();
    let mut from = 0;
    let mut cursor = 0;

    for run in common_runs(before, after) {
        if run.before > from || run.after > cursor {
            regions.push(ChangedRegion {
                from,
                to: run.before,
                lines: after[cursor..run.after].to_vec(),
            });
        }
        from = run.before + run.length;
        cursor = run.after + run.length;
    }

    if from < before.len() || cursor < after.len() {
        regions.push(ChangedRegion {
            from,
            to: before.len(),
            lines: after[cursor..].to_vec(),
        });
    }

    regions
}

/// A maximal run of lines the two versions agree on, at `before` and `after` respectively.
struct CommonRun {
    pub(crate) before: usize,
    pub(crate) after: usize,
    pub(crate) length: usize,
}

/// The common runs of a shortest edit script, by Myers' algorithm.
///
/// The search walks diagonals of the edit graph, recording the furthest point reached on each one at
/// every edit distance; the recorded frontiers are then walked backwards to recover the path, whose
/// diagonal moves are exactly the lines the two versions share.
///
/// A path is always found within `n + m` edits — delete everything, then insert everything — so the
/// loop cannot fall through, and the empty case is answered before the frontier is sized.
fn common_runs(before: &[&str], after: &[&str]) -> Vec<CommonRun> {
    let n = before.len() as isize;
    let m = after.len() as isize;
    let max = n + m;
    if max == 0 {
        return Vec::new();
    }

    let offset = max;
    let at = |k: isize| (k + offset) as usize;
    let mut frontier = vec![0isize; (2 * max + 1) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();

    for d in 0..=max {
        trace.push(frontier.clone());

        let mut k = -d;
        while k <= d {
            // The furthest-reaching path on this diagonal arrives either from the one below or from
            // the one above; `&&` short-circuits so the out-of-range neighbour is never read.
            let go_down = k == -d || (k != d && frontier[at(k - 1)] < frontier[at(k + 1)]);
            let mut x = if go_down {
                frontier[at(k + 1)]
            } else {
                frontier[at(k - 1)] + 1
            };
            let mut y = x - k;

            while x < n && y < m && before[x as usize] == after[y as usize] {
                x += 1;
                y += 1;
            }

            frontier[at(k)] = x;

            if x >= n && y >= m {
                return backtrack(&trace, n, m, offset);
            }
            k += 2;
        }
    }

    unreachable!("a shortest edit script always exists within {max} edits")
}

/// Walks the recorded frontiers backwards, collecting the diagonal moves of the shortest path.
fn backtrack(trace: &[Vec<isize>], n: isize, m: isize, offset: isize) -> Vec<CommonRun> {
    let at = |k: isize| (k + offset) as usize;
    let mut runs = Vec::new();
    let mut x = n;
    let mut y = m;

    for step in (1..trace.len()).rev() {
        let frontier = &trace[step];
        let d = step as isize;
        let k = x - y;
        let go_down = k == -d || (k != d && frontier[at(k - 1)] < frontier[at(k + 1)]);
        let previous_k = if go_down { k + 1 } else { k - 1 };
        let previous_x = frontier[at(previous_k)];
        let previous_y = previous_x - previous_k;

        let mut length = 0;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            length += 1;
        }
        if length > 0 {
            runs.push(CommonRun {
                before: x as usize,
                after: y as usize,
                length,
            });
        }

        x = previous_x;
        y = previous_y;
    }

    if x > 0 {
        runs.push(CommonRun {
            before: 0,
            after: 0,
            length: x as usize,
        });
    }

    runs.reverse();
    runs
}
