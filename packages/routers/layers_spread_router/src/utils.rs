// use std::collections::HashMap;
//
// pub fn closest_to<T>(value: u8, map: &HashMap<u8, T>) -> Option<u8> {
//     let mut min_index: Option<u8> = None;
//     for (index, _) in map {
//         match min_index {
//             None => {
//                 min_index = Some(*index);
//             },
//             Some(prev) => {
//                 let prev_dis = circle_distance(value, prev);
//                 let new_dis = circle_distance(value, *index);
//                 if new_dis < prev_dis || ( new_dis == prev_dis && *index < prev ) {
//                     min_index = Some(*index);
//                 }
//             }
//         };
//     }
//
//     min_index
// }
//
// pub fn closest_to_sorted_list(key: u8, sorted_list: &[u8]) -> Option<u8> {
//     if sorted_list.is_empty() {
//         return None;
//     }
//
//     match sorted_list.binary_search(&key) {
//         Ok(_) => { //found => using provided slot
//             Some(key)
//         }
//         Err(bigger_index) => {
//             let (left, right) = {
//                 if bigger_index < sorted_list.len() { //found slot that bigger than key
//                     if bigger_index > 0 {
//                         (bigger_index - 1, bigger_index)
//                     } else {
//                         (sorted_list.len() - 1, bigger_index)
//                     }
//                 } else {
//                     (bigger_index - 1, 0)
//                 }
//             };
//
//             let left_value = sorted_list[left];
//             let right_value = sorted_list[right];
//             if circle_distance(left_value, key) <= circle_distance(right_value, key) {
//                 Some(left_value)
//             } else {
//                 Some(right_value)
//             }
//         }
//     }
// }
//
// #[cfg(test)]
// mod tests {
//     use std::collections::HashMap;
//     use super::closest_to;
//     use super::closest_to_sorted_list;
//
//     fn test_for_key(slots: Vec<u8>, tests: Vec<(u8, Option<u8>)>) {
//         let mut map = HashMap::new();
//         for slot in slots {
//             map.insert(slot, true);
//         }
//
//         for (request, expected) in tests {
//             assert_eq!(closest_to(request, &map), expected);
//         }
//     }
//
//     #[test]
//     fn node_for_key_tests() {
//         test_for_key(vec![], vec![(0, None), (100, None)]);
//         test_for_key(vec![1, 5, 40], vec![(3, Some(1)), (4, Some(5)), (0, Some(1)), (41, Some(40)), (255, Some(1))]);
//         test_for_key(vec![24, 120], vec![(25, Some(24)), (23, Some(24)), (88, Some(120)), (152, Some(120)), (216, Some(24))]);
//
//         assert_eq!(closest_to_sorted_list(25, &[24, 120]), Some(24));
//         assert_eq!(closest_to_sorted_list(3, &[1,2,3,4,5]), Some(3));
//         assert_eq!(closest_to_sorted_list(6, &[1,2,3,4,5] ), Some(5));
//         assert_eq!(closest_to_sorted_list(0, &[1,2,3,4,5]), Some(1));
//     }
// }
