//! Reads back the `.usda` layers the exporter writes. This is not a USD parser: it
//! relies on the writer's layout (one statement per line, four spaces per level) and
//! checks that layout as it goes, so a change in the writer shows up here first.

use limner_core::glam::{DAffine3, DMat3, DQuat, DVec3, Vec3};

pub struct Usda {
    lines: Vec<String>,
}

/// One `def` in the layer.
#[derive(Clone, Copy)]
pub struct Prim<'a> {
    usda: &'a Usda,
    /// Line of the `def`.
    start: usize,
    /// Line of the opening brace.
    open: usize,
    /// Line of the closing brace.
    end: usize,
    depth: usize,
}

fn indent_of(line: &str) -> usize {
    let spaces = line.len() - line.trim_start_matches(' ').len();
    assert_eq!(
        spaces % 4,
        0,
        "indentation is four spaces per level: {line:?}"
    );
    spaces / 4
}

/// Every number in a value, in order.
pub fn numbers(value: &str) -> Vec<f64> {
    value
        .split(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '.' | 'e' | 'E' | '+')))
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap_or_else(|_| panic!("not a number: {s:?}")))
        .collect()
}

pub fn vec3s(value: &str) -> Vec<Vec3> {
    let flat = numbers(value);
    assert_eq!(flat.len() % 3, 0, "triples");
    flat.as_chunks::<3>()
        .0
        .iter()
        .map(|c| Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32))
        .collect()
}

/// A quoted string's content.
pub fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or_else(|| panic!("not a quoted string: {value:?}"))
}

/// Splits `value (metadata)` into the value and what is inside the parentheses.
fn split_metadata(rest: &str) -> (&str, Option<&str>) {
    let close = match rest.as_bytes().first() {
        Some(b'[') => rest.rfind(']'),
        Some(b'"') => rest[1..].find('"').map(|i| i + 1),
        Some(b'(') => {
            // A tuple; a metadata block may follow it.
            let mut depth = 0;
            rest.char_indices().find_map(|(i, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                (depth == 0).then_some(i)
            })
        }
        Some(b'<') => rest.find('>'),
        _ => rest.find(' ').map(|i| i - 1).or(Some(rest.len() - 1)),
    }
    .unwrap_or_else(|| panic!("unterminated value: {rest:?}"));
    let (value, tail) = rest.split_at(close + 1);
    let tail = tail.trim();
    if tail.is_empty() {
        (value, None)
    } else {
        let inner = tail
            .strip_prefix('(')
            .and_then(|t| t.strip_suffix(')'))
            .unwrap_or_else(|| panic!("unexpected text after a value: {tail:?}"));
        (value, Some(inner.trim()))
    }
}

impl Usda {
    /// Splits the layer into lines and checks the header and that every bracket closes.
    pub fn parse(text: &str) -> Usda {
        assert!(text.starts_with("#usda 1.0\n"), "the usda magic line");
        assert!(text.ends_with('\n'), "the layer ends with a newline");
        assert!(!text.contains('\t') && !text.contains('\r'));
        let mut stack = Vec::new();
        let mut in_string = false;
        let mut escaped = false;
        for c in text.chars() {
            if in_string {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    '\n' => panic!("a string runs over the end of its line"),
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => stack.push(c),
                ')' | ']' | '}' => {
                    let open = stack.pop().expect("a closing bracket has an opening one");
                    assert_eq!(
                        (open, c),
                        match c {
                            ')' => ('(', ')'),
                            ']' => ('[', ']'),
                            _ => ('{', '}'),
                        },
                        "brackets nest"
                    );
                }
                _ => {}
            }
        }
        assert!(!in_string && stack.is_empty(), "everything is closed");
        let usda = Usda {
            lines: text.lines().map(str::to_owned).collect(),
        };
        for line in &usda.lines {
            indent_of(line);
        }
        usda
    }

    /// The value of a field in the layer's metadata block.
    pub fn layer_metadata(&self, key: &str) -> Option<&str> {
        assert_eq!(
            self.lines[1], "(",
            "the layer metadata follows the magic line"
        );
        let prefix = format!("    {key} = ");
        self.lines[2..]
            .iter()
            .take_while(|line| line.as_str() != ")")
            .find_map(|line| line.strip_prefix(prefix.as_str()))
    }

    fn prim_at(&self, start: usize) -> Prim<'_> {
        let line = &self.lines[start];
        let depth = indent_of(line);
        let pad = " ".repeat(depth * 4);
        let mut open = start + 1;
        if line.ends_with('(') {
            while self.lines[open] != format!("{pad})") {
                open += 1;
            }
            open += 1;
        }
        assert_eq!(
            self.lines[open],
            format!("{pad}{{"),
            "a prim opens its body"
        );
        let close = format!("{pad}}}");
        let end = (open + 1..self.lines.len())
            .find(|i| self.lines[*i] == close)
            .expect("a prim closes its body");
        Prim {
            usda: self,
            start,
            open,
            end,
            depth,
        }
    }

    /// The prims at the top of the layer.
    pub fn roots(&self) -> Vec<Prim<'_>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.lines.len() {
            if self.lines[i].starts_with("def ") {
                let prim = self.prim_at(i);
                i = prim.end;
                out.push(prim);
            }
            i += 1;
        }
        out
    }

    /// The prim at an absolute path such as `/Stage/Root/Rig`.
    pub fn prim(&self, path: &str) -> Prim<'_> {
        self.find(path)
            .unwrap_or_else(|| panic!("no prim at {path}"))
    }

    pub fn find(&self, path: &str) -> Option<Prim<'_>> {
        let mut parts = path.strip_prefix('/').expect("an absolute path").split('/');
        let first = parts.next()?;
        let mut prim = self.roots().into_iter().find(|p| p.name() == first)?;
        for part in parts {
            prim = prim.child(part)?;
        }
        Some(prim)
    }

    /// The default prim, which must exist and be at the top.
    pub fn default_prim(&self) -> Prim<'_> {
        let name = unquote(self.layer_metadata("defaultPrim").expect("a defaultPrim"));
        self.prim(&format!("/{name}"))
    }

    /// Every prim in the layer with its absolute path.
    pub fn all(&self) -> Vec<(String, Prim<'_>)> {
        fn walk<'a>(prim: Prim<'a>, parent: &str, out: &mut Vec<(String, Prim<'a>)>) {
            let path = format!("{parent}/{}", prim.name());
            out.push((path.clone(), prim));
            for child in prim.children() {
                walk(child, &path, out);
            }
        }
        let mut out = Vec::new();
        for root in self.roots() {
            walk(root, "", &mut out);
        }
        out
    }
}

impl<'a> Prim<'a> {
    fn header(&self) -> (&'a str, &'a str) {
        let line = self.usda.lines[self.start].trim_start();
        let rest = line.strip_prefix("def ").expect("a def");
        let (type_name, rest) = rest.split_once(' ').expect("a type and a name");
        let name = rest
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .expect("a quoted name")
            .0;
        (type_name, name)
    }

    pub fn type_name(&self) -> &'a str {
        self.header().0
    }

    pub fn name(&self) -> &'a str {
        self.header().1
    }

    /// The lines of the prim's metadata block, trimmed.
    pub fn metadata(&self) -> Vec<&'a str> {
        // Without a block the brace follows the `def` directly; with one, the block's
        // closing parenthesis is the line before the brace.
        let end = (self.open - 1).max(self.start + 1);
        self.usda.lines[self.start + 1..end]
            .iter()
            .map(|line| line.trim())
            .collect()
    }

    /// The value of a `type key = value` or `key = value` line anywhere in the metadata.
    pub fn metadata_value(&self, key: &str) -> Option<&'a str> {
        let needle = format!("{key} = ");
        self.metadata().into_iter().find_map(|line| {
            let at = line.find(needle.as_str())?;
            let before = &line[..at];
            (before.is_empty() || before.ends_with(' ')).then(|| &line[at + needle.len()..])
        })
    }

    pub fn children(&self) -> Vec<Prim<'a>> {
        let mut out = Vec::new();
        let mut i = self.open + 1;
        while i < self.end {
            if self.usda.lines[i].trim_start().starts_with("def ") {
                let child = self.usda.prim_at(i);
                assert_eq!(child.depth, self.depth + 1);
                i = child.end;
                out.push(child);
            }
            i += 1;
        }
        out
    }

    pub fn child(&self, name: &str) -> Option<Prim<'a>> {
        self.children().into_iter().find(|c| c.name() == name)
    }

    /// The prim's own property lines: `(declaration, rest of the line, line index)`.
    fn properties(&self) -> Vec<(&'a str, &'a str, usize)> {
        let pad = " ".repeat((self.depth + 1) * 4);
        let mut out = Vec::new();
        let mut i = self.open + 1;
        while i < self.end {
            let line = self.usda.lines[i].as_str();
            let body = line.strip_prefix(pad.as_str()).unwrap_or("");
            if body.starts_with("def ") {
                i = self.usda.prim_at(i).end + 1;
                continue;
            }
            if !body.is_empty() && !body.starts_with(' ') && body != "}" {
                let (declaration, rest) = body.split_once(" = ").unwrap_or((body, ""));
                out.push((declaration, rest, i));
            }
            i += 1;
        }
        out
    }

    fn property(&self, name: &str) -> Option<(&'a str, &'a str, usize)> {
        self.properties().into_iter().find(|(declaration, _, _)| {
            *declaration == name || declaration.ends_with(&format!(" {name}"))
        })
    }

    /// Names of the prim's own properties, with `.timeSamples` and `.connect` kept.
    pub fn property_names(&self) -> Vec<&'a str> {
        self.properties()
            .into_iter()
            .map(|(declaration, _, _)| declaration.rsplit(' ').next().unwrap())
            .collect()
    }

    /// The default value of a property, without any trailing metadata.
    pub fn attr(&self, name: &str) -> Option<&'a str> {
        self.property(name)
            .map(|(_, rest, _)| split_metadata(rest).0)
    }

    /// The declared type of a property, including `uniform` when present.
    pub fn attr_type(&self, name: &str) -> Option<&'a str> {
        self.property(name)
            .map(|(declaration, _, _)| declaration.rsplit_once(' ').expect("a type").0)
    }

    /// What follows a property's value in parentheses, such as `interpolation = "vertex"`.
    pub fn attr_metadata(&self, name: &str) -> Option<&'a str> {
        self.property(name)
            .and_then(|(_, rest, _)| split_metadata(rest).1)
    }

    /// The target of a relationship or connection, without the angle brackets.
    pub fn target(&self, name: &str) -> Option<&'a str> {
        self.attr(name).map(|value| {
            value
                .strip_prefix('<')
                .and_then(|v| v.strip_suffix('>'))
                .unwrap_or_else(|| panic!("not a path: {value:?}"))
        })
    }

    /// The time samples of a property: `(time code, value)` in file order.
    pub fn time_samples(&self, name: &str) -> Option<Vec<(f64, &'a str)>> {
        let (_, rest, line) = self.property(&format!("{name}.timeSamples"))?;
        assert_eq!(rest, "{", "time samples open a block");
        let close = format!("{}}}", " ".repeat((self.depth + 1) * 4));
        let mut out: Vec<(f64, &'a str)> = Vec::new();
        for sample in &self.usda.lines[line + 1..] {
            if *sample == close {
                break;
            }
            let (time, value) = sample.trim().split_once(": ").expect("time: value");
            let value = value.strip_suffix(',').expect("samples end with a comma");
            let time: f64 = time.parse().expect("a time code");
            if let Some(previous) = out.last() {
                assert!(previous.0 < time, "time codes increase");
            }
            out.push((time, value));
        }
        assert!(!out.is_empty(), "a time-sampled property has samples");
        Some(out)
    }

    /// The property's value at a time code: the held sample when `held`, otherwise
    /// linear between the two samples around it (component-wise).
    pub fn sample_numbers(&self, name: &str, time: f64, held: bool) -> Option<Vec<f64>> {
        let Some(samples) = self.time_samples(name) else {
            return self.attr(name).map(numbers);
        };
        let after = samples.partition_point(|(t, _)| *t <= time);
        if after == 0 {
            return Some(numbers(samples[0].1));
        }
        let (t0, v0) = samples[after - 1];
        if held || after == samples.len() || t0 == time {
            return Some(numbers(v0));
        }
        let (t1, v1) = samples[after];
        let u = (time - t0) / (t1 - t0);
        Some(
            numbers(v0)
                .iter()
                .zip(numbers(v1))
                .map(|(a, b)| a + (b - a) * u)
                .collect(),
        )
    }

    /// Whether the prim's own `visibility` shows it at a time code.
    pub fn visible_at(&self, time: f64) -> bool {
        let token = match self.time_samples("visibility") {
            Some(samples) => {
                let after = samples.partition_point(|(t, _)| *t <= time);
                samples[after.saturating_sub(1)].1
            }
            None => self.attr("visibility").unwrap_or("\"inherited\""),
        };
        match unquote(token) {
            "inherited" => true,
            "invisible" => false,
            other => panic!("visibility is inherited or invisible, not {other}"),
        }
    }

    /// The prim's local matrix at a time code, from its `xformOpOrder`.
    pub fn local_matrix(&self, time: f64) -> DAffine3 {
        let Some(order) = self.attr("xformOpOrder") else {
            for op in ["translate", "orient", "scale"] {
                assert!(
                    self.attr(&format!("xformOp:{op}")).is_none(),
                    "an op that is not in xformOpOrder does nothing"
                );
            }
            return DAffine3::IDENTITY;
        };
        assert_eq!(self.attr_type("xformOpOrder"), Some("uniform token[]"));
        let mut m = DAffine3::IDENTITY;
        for op in order
            .trim_matches(|c| c == '[' || c == ']')
            .split(", ")
            .map(unquote)
        {
            let v = self
                .sample_numbers(op, time, false)
                .unwrap_or_else(|| panic!("{op} is listed but not authored"));
            m *= match op {
                "xformOp:translate" => DAffine3::from_translation(DVec3::new(v[0], v[1], v[2])),
                // USD writes quaternions real part first.
                "xformOp:orient" => {
                    DAffine3::from_quat(DQuat::from_xyzw(v[1], v[2], v[3], v[0]).normalize())
                }
                "xformOp:scale" => {
                    DAffine3::from_mat3(DMat3::from_diagonal(DVec3::new(v[0], v[1], v[2])))
                }
                other => panic!("unexpected xform op {other}"),
            };
        }
        m
    }
}

impl Usda {
    /// The placement in world space of the prim at `path`, at a time code.
    pub fn world_matrix(&self, path: &str, time: f64) -> DAffine3 {
        let mut m = DAffine3::IDENTITY;
        let mut current = String::new();
        for part in path.strip_prefix('/').expect("an absolute path").split('/') {
            current.push('/');
            current.push_str(part);
            m *= self.prim(&current).local_matrix(time);
        }
        m
    }

    /// True when the prim at `path` and all its ancestors are visible at a time code.
    pub fn is_visible(&self, path: &str, time: f64) -> bool {
        let mut current = String::new();
        path.strip_prefix('/')
            .expect("an absolute path")
            .split('/')
            .all(|part| {
                current.push('/');
                current.push_str(part);
                self.prim(&current).visible_at(time)
            })
    }
}
