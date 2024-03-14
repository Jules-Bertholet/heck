use std::{
    error::Error,
    io::{self},
    mem::size_of,
};

use regex::Regex;
use rustc_hash::FxHashMap;

use crate::unicode_data::DataFiles;

fn titlecases(data: &DataFiles) -> Vec<(char, Vec<char>)> {
    let mut map = FxHashMap::default();

    // Single character mappings
    let regex = Regex::new(
        r"^([0-9A-F]+);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);(?:.*?);([0-9A-F]*);(?:.*?);([0-9A-F]+)",
    ).unwrap();
    for line in data.unicode_data.lines() {
        if let Some(captures) = regex.captures(line) {
            if let Some(titlecase) = captures.get(3) {
                // Only include if different from uppercase
                if titlecase.as_str() != &captures[2] {
                    let cp =
                        char::from_u32(u32::from_str_radix(&captures[1], 16).unwrap()).unwrap();
                    let titlecase_cp =
                        char::from_u32(u32::from_str_radix(titlecase.as_str(), 16).unwrap())
                            .unwrap();
                    assert!(!map.contains_key(&cp));
                    map.insert(cp, vec![titlecase_cp]);
                }
            }
        }
    }

    // Multi character mappings
    let regex =
        Regex::new(r"^([0-9A-F]+);(?:[0-9A-F ]*);([0-9A-F ]*);([0-9A-F ]*);[^0-9A-Fa-f_]*#")
            .unwrap();
    for line in data.special_casing.lines() {
        if let Some(captures) = regex.captures(line) {
            let titlecase_mapping = captures[2].trim();
            let uppercase_mapping = captures[3].trim();
            if titlecase_mapping != uppercase_mapping {
                let cp = char::from_u32(u32::from_str_radix(&captures[1], 16).unwrap()).unwrap();
                assert!(!map.contains_key(&cp));
                map.insert(
                    cp,
                    titlecase_mapping
                        .split_whitespace()
                        .map(|s| char::from_u32(u32::from_str_radix(s, 16).unwrap()).unwrap())
                        .collect(),
                );
            }
        }
    }

    let mut vec: Vec<(char, Vec<char>)> = map.into_iter().collect();
    vec.sort_unstable_by_key(|(c, _)| *c);
    vec
}

pub fn write_table(out: &mut impl io::Write, data: &DataFiles) -> Result<(), Box<dyn Error>> {
    let titlecase_mappings = titlecases(data);
    let max_expansion = titlecase_mappings.iter().map(|t| t.1.len()).max().unwrap();

    eprintln!(
        "titlecase: {} bytes of static data",
        (max_expansion + 1) * size_of::<char>() * titlecase_mappings.len()
    );

    writeln!(
        out,
        "
use core::{{
    array,
    char::ToUppercase,
    fmt::{{self, Write}},
    iter,
}};

#[derive(Clone, Debug)]
enum TitlecaseIter {{
    Titlecase(iter::Flatten<array::IntoIter<Option<char>, 3>>),
    Uppercase(ToUppercase),
}}

impl Iterator for TitlecaseIter {{
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {{
        match self {{
            Self::Titlecase(i) => i.next(),
            Self::Uppercase(i) => i.next(),
        }}
    }}

    fn size_hint(&self) -> (usize, Option<usize>) {{
        match self {{
            Self::Titlecase(i) => i.size_hint(),
            Self::Uppercase(i) => i.size_hint(),
        }}
    }}
}}

impl fmt::Display for TitlecaseIter {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
        for c in self.clone() {{
            f.write_char(c)?;
        }}
        Ok(())
    }}
}}

/// Returns an iterator that yields the titlecase mapping of this `char` as one or more `char`s.
pub fn to_titlecase(c: char) -> impl Iterator<Item = char> + fmt::Display {{
    if let Ok(idx) = TITLECASE_MAPPINGS.binary_search_by_key(&c, |&(c2, _)| c2) {{
        TitlecaseIter::Titlecase(TITLECASE_MAPPINGS[idx].1.into_iter().flatten())
    }} else {{
        TitlecaseIter::Uppercase(c.to_uppercase())
    }}
}}

/// Sorted list of characters and their titlecase mappings.
/// Only characters whose titlecase differs from uppercase are included.
static TITLECASE_MAPPINGS: [(char, [Option<char>; {max_expansion}]); {}] = [",
        titlecase_mappings.len()
    )?;
    for (c, mapping) in titlecase_mappings {
        write!(out, "    ('{c}', [")?;

        let mut mapping = mapping.into_iter();

        if let Some(fc) = mapping.next() {
            write!(out, "Some('{fc}')")?;
        } else {
            write!(out, "None")?;
        }

        for _ in 1..max_expansion {
            if let Some(c) = mapping.next() {
                write!(out, ", Some('{c}')")?;
            } else {
                write!(out, ", None")?;
            }
        }

        writeln!(out, "]),")?;
    }
    writeln!(out, "];")?;

    Ok(())
}
