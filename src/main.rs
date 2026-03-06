// Jackson Coxson

use jkcli::{JkArgument, JkCommand};
use owo_colors::OwoColorize;
use plist::Value;
use plist_macro::pretty_print_plist;

fn main() {
    let command = JkCommand::new()
        .help("plist_diff - get the difference between plists")
        .with_argument(
            JkArgument::new()
                .required(true)
                .with_help("reference plist"),
        )
        .with_argument(
            JkArgument::new()
                .required(true)
                .with_help("comparison plist"),
        );

    let mut args = command.collect().expect("Failed to collect args");

    let ref_p: String = args.next_argument().unwrap(); // we can unwrap because it's required
    let com_p: String = args.next_argument().unwrap();

    let ref_p: Value = plist::from_file(ref_p).expect("Failed to read ref plist");
    let com_p: Value = plist::from_file(com_p).expect("Failed to read comparison plist");

    // Iterate through the plist and check for digressions
    // We don't care if values are in the same order since some internal tooling alphabetizes the
    // plist keys :eyeroll:

    let mut current_path = vec!["root".to_string()];
    if do_cmp(&ref_p, &com_p, &mut current_path) {
        println!("plists are equivalent");
    }
}

fn same_variant(a: &Value, b: &Value) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

// Returns a score representing how similar two values are.
// Higher is more similar; usize::MAX means identical.
fn similarity_score(a: &Value, b: &Value) -> usize {
    if a == b {
        return usize::MAX;
    }
    match (a, b) {
        (Value::Dictionary(da), Value::Dictionary(db)) => {
            da.iter()
                .filter(|(k, v)| db.get(*k).map_or(false, |v2| v2 == *v))
                .count()
        }
        (Value::Array(aa), Value::Array(ab)) => {
            let mut matched = vec![false; ab.len()];
            let mut count = 0;
            for item in aa {
                if let Some(i) = matched
                    .iter()
                    .enumerate()
                    .find(|(i, used)| !**used && &ab[*i] == item)
                    .map(|(i, _)| i)
                {
                    matched[i] = true;
                    count += 1;
                }
            }
            count
        }
        _ => 0,
    }
}

// returns whether the plist is equivalent or not
fn do_cmp(ref_p: &Value, com_p: &Value, current_path: &mut Vec<String>) -> bool {
    // first sanity check
    if ref_p == com_p {
        return true;
    }

    match &ref_p {
        Value::Dictionary(dictionary) => {
            if let plist::Value::Dictionary(com_p) = com_p {
                let mut is_still_equal = true;
                for (k, v) in dictionary {
                    if let Some(v2) = com_p.get(k) {
                        let mut current_path = current_path.clone();
                        current_path.push(k.to_string());
                        if !do_cmp(v, v2, &mut current_path) {
                            is_still_equal = false;
                        }
                    } else {
                        println!("{}: missing key {}", current_path.join("/").blue(), k.red());
                        is_still_equal = false;
                    }
                }

                // see if cmp has any extra keys
                if com_p.len() > dictionary.len() {
                    for k in com_p.keys() {
                        if !dictionary.contains_key(k) {
                            println!("{}: extra key {}", current_path.join("/").blue(), k.red());
                            is_still_equal = false;
                        }
                    }
                }

                is_still_equal
            } else {
                println!(
                    "{}: {} - {}",
                    current_path.join("/").blue(),
                    pretty_print_plist(ref_p).red(),
                    pretty_print_plist(com_p).red()
                );
                false
            }
        }
        Value::Array(ref_arr) => {
            if let Value::Array(com_arr) = com_p {
                let mut is_still_equal = true;
                let mut com_matched = vec![false; com_arr.len()];
                let mut ref_matched = vec![false; ref_arr.len()];

                // First pass: exact matches — no output needed
                for (ri, ref_item) in ref_arr.iter().enumerate() {
                    if let Some(ci) = com_matched
                        .iter()
                        .enumerate()
                        .find(|(ci, used)| !**used && &com_arr[*ci] == ref_item)
                        .map(|(ci, _)| ci)
                    {
                        com_matched[ci] = true;
                        ref_matched[ri] = true;
                    }
                }

                // Second pass: for each unmatched ref item, find the most similar unmatched
                // com item of the same type and recurse into it for a detailed diff.
                for (ri, ref_item) in ref_arr.iter().enumerate() {
                    if ref_matched[ri] {
                        continue;
                    }
                    let best = com_matched
                        .iter()
                        .enumerate()
                        .filter(|(ci, used)| !**used && same_variant(ref_item, &com_arr[*ci]))
                        .max_by_key(|(ci, _)| similarity_score(ref_item, &com_arr[*ci]))
                        .map(|(ci, _)| ci);

                    if let Some(ci) = best {
                        com_matched[ci] = true;
                        let mut child_path = current_path.clone();
                        child_path.push(format!("[{}]", ri));
                        if !do_cmp(ref_item, &com_arr[ci], &mut child_path) {
                            is_still_equal = false;
                        }
                    } else {
                        println!(
                            "{}: missing array element {}",
                            current_path.join("/").blue(),
                            pretty_print_plist(ref_item).red()
                        );
                        is_still_equal = false;
                    }
                }

                // Report extra com elements
                for (i, used) in com_matched.iter().enumerate() {
                    if !used {
                        println!(
                            "{}: extra array element {}",
                            current_path.join("/").blue(),
                            pretty_print_plist(&com_arr[i]).red()
                        );
                        is_still_equal = false;
                    }
                }

                is_still_equal
            } else {
                println!(
                    "{}: {} - {}",
                    current_path.join("/").blue(),
                    pretty_print_plist(ref_p).red(),
                    pretty_print_plist(com_p).red()
                );
                false
            }
        }
        _ => {
            println!(
                "{}: {} - {}",
                current_path.join("/").blue(),
                pretty_print_plist(ref_p).red(),
                pretty_print_plist(com_p).red()
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist_macro::plist;

    // ── Primitives ────────────────────────────────────────────────────────────

    #[test]
    fn identical_booleans() {
        let mut path = vec!["root".to_string()];
        assert!(do_cmp(&plist!(true), &plist!(true), &mut path));
        assert!(do_cmp(&plist!(false), &plist!(false), &mut path));
    }

    #[test]
    fn differing_booleans() {
        let mut path = vec!["root".to_string()];
        assert!(!do_cmp(&plist!(true), &plist!(false), &mut path));
    }

    #[test]
    fn identical_integers() {
        let mut path = vec!["root".to_string()];
        assert!(do_cmp(&plist!(42), &plist!(42), &mut path));
    }

    #[test]
    fn differing_integers() {
        let mut path = vec!["root".to_string()];
        assert!(!do_cmp(&plist!(1), &plist!(2), &mut path));
    }

    #[test]
    fn identical_strings() {
        let mut path = vec!["root".to_string()];
        assert!(do_cmp(&plist!("hello"), &plist!("hello"), &mut path));
    }

    #[test]
    fn differing_strings() {
        let mut path = vec!["root".to_string()];
        assert!(!do_cmp(&plist!("foo"), &plist!("bar"), &mut path));
    }

    #[test]
    fn type_mismatch_bool_vs_integer() {
        let mut path = vec!["root".to_string()];
        // true != 1 in plist land
        assert!(!do_cmp(&plist!(true), &plist!(1), &mut path));
    }

    #[test]
    fn type_mismatch_string_vs_integer() {
        let mut path = vec!["root".to_string()];
        assert!(!do_cmp(&plist!("42"), &plist!(42), &mut path));
    }

    // ── Flat dictionaries ─────────────────────────────────────────────────────

    #[test]
    fn identical_flat_dicts() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "hi mom": 123, "yes": true });
        let p2 = plist!({ "hi mom": 123, "yes": true });
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn flat_dict_key_order_irrelevant() {
        // Keys inserted in different order should still be equal.
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1, "b": 2 });
        let p2 = plist!({ "b": 2, "a": 1 });
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn flat_dict_missing_key_in_comparison() {
        // ref has "extra_key" that com lacks → not equal
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1, "b": 2 });
        let p2 = plist!({ "a": 1 });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn flat_dict_extra_key_in_comparison() {
        // com has a key that ref does not → not equal
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1 });
        let p2 = plist!({ "a": 1, "b": 2 });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn flat_dict_value_mismatch() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1 });
        let p2 = plist!({ "a": 99 });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn empty_dicts_are_equal() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({});
        let p2 = plist!({});
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    // ── Nested dictionaries ───────────────────────────────────────────────────

    #[test]
    fn nested_dicts_identical() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "outer": {
                "inner": 42
            }
        });
        let p2 = plist!({
            "outer": {
                "inner": 42
            }
        });
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn nested_dict_inner_value_differs() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "outer": {
                "inner": 1
            }
        });
        let p2 = plist!({
            "outer": {
                "inner": 2
            }
        });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn nested_dict_inner_key_missing() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "outer": {
                "inner": 1,
                "also": true
            }
        });
        let p2 = plist!({
            "outer": {
                "inner": 1
            }
        });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn deeply_nested_dicts_identical() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "l1": {
                "l2": {
                    "l3": "deep"
                }
            }
        });
        let p2 = plist!({
            "l1": {
                "l2": {
                    "l3": "deep"
                }
            }
        });
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn deeply_nested_dicts_differ() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "l1": {
                "l2": {
                    "l3": "deep"
                }
            }
        });
        let p2 = plist!({
            "l1": {
                "l2": {
                    "l3": "shallow"
                }
            }
        });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn dict_vs_non_dict() {
        // ref is a dict, com is a scalar → type mismatch
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1 });
        let p2 = plist!(true);
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    // ── Arrays ────────────────────────────────────────────────────────────────

    #[test]
    fn identical_arrays() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([1, 2, 3]);
        let p2 = plist!([1, 2, 3]);
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn arrays_order_irrelevant() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([1, 2, 3]);
        let p2 = plist!([3, 1, 2]);
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn differing_arrays() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([1, 2, 3]);
        let p2 = plist!([1, 2, 4]);
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn arrays_different_lengths() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([1, 2]);
        let p2 = plist!([1, 2, 3]);
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn empty_arrays_equal() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([]);
        let p2 = plist!([]);
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn array_of_dicts_nested_diff() {
        // Dicts in different order; the one with "a":1 has a differing "b" value
        let mut path = vec!["root".to_string()];
        let p1 = plist!([{ "a": 1, "b": 10 }, { "a": 2, "b": 20 }]);
        let p2 = plist!([{ "a": 2, "b": 20 }, { "a": 1, "b": 99 }]);
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn array_of_dicts_identical_unordered() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!([{ "a": 1, "b": 10 }, { "a": 2, "b": 20 }]);
        let p2 = plist!([{ "a": 2, "b": 20 }, { "a": 1, "b": 10 }]);
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    // ── Mixed / realistic structures ──────────────────────────────────────────

    #[test]
    fn realistic_plist_equal() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "CFBundleVersion": "1.0.0",
            "CFBundleShortVersionString": "1.0",
            "NSAppTransportSecurity": {
                "NSAllowsArbitraryLoads": true
            },
            "UIRequiredDeviceCapabilities": ["armv7"]
        });
        let p2 = plist!({
            "CFBundleVersion": "1.0.0",
            "CFBundleShortVersionString": "1.0",
            "NSAppTransportSecurity": {
                "NSAllowsArbitraryLoads": true
            },
            "UIRequiredDeviceCapabilities": ["armv7"]
        });
        assert!(do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn realistic_plist_version_differs() {
        let mut path = vec!["root".to_string()];
        let p1 = plist!({
            "CFBundleVersion": "1.0.0",
            "CFBundleShortVersionString": "1.0"
        });
        let p2 = plist!({
            "CFBundleVersion": "2.0.0",
            "CFBundleShortVersionString": "1.0"
        });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }

    #[test]
    fn multiple_differences_all_reported() {
        // Even with multiple mismatched keys, the function should return false
        // (and not short-circuit after the first miss).
        let mut path = vec!["root".to_string()];
        let p1 = plist!({ "a": 1, "b": 2, "c": 3 });
        let p2 = plist!({ "a": 9, "b": 9, "c": 3 });
        assert!(!do_cmp(&p1, &p2, &mut path));
    }
}
