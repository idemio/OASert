


#[derive(Debug, Clone)]
pub struct JsonPath(pub Vec<String>);

impl JsonPath {
    pub fn new() -> Self {
        JsonPath(Vec::new())
    }

    pub fn add_segment(&mut self, segment: String) -> &mut Self {
        if segment.contains("/") {
            let segment = segment.replace("/", "~1");
            self.0.push(segment);
        } else {
            self.0.push(segment);
        }
        self
    }

    pub fn append_path(&mut self, path: JsonPath) -> &mut Self {
        let mut path = path;
        self.0.append(&mut path.0);
        self
    }

    pub fn format_path(&self) -> String {
        self.0.join("/")
    }
}

