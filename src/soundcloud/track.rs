#[derive(Clone, Debug)]
pub struct Track {
    id: String,
    title: String,
}

impl Track {
    pub fn new_test() -> Track {
        Track {
            id: "seantyas_lift".to_string(),
            title: "Sean Tyas - Lift".to_string(),
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }
}
