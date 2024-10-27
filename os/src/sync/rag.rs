
struct ResourceAllocationGraph {
    // Resource instance to process allocation relationship
    resource_to_process: BTreeMap<Resource, Process>,
    // Process to resource instance request relationship
    process_to_resource: BTreeMap<Process, BTreeSet<Resource>>,
}

impl ResourceAllocationGraph {
    fn new() -> Self {
        ResourceAllocationGraph {
            resource_to_process: BTreeMap::new(),
            process_to_resource: BTreeMap::new(),
        }
    }

    fn request_resource(&mut self, process: Process, resource: Resource) {
        self.process_to_resource
            .entry(process.clone())
            .or_insert_with(BTreeSet::new)
            .insert(resource.clone());

        self.resource_to_process.insert(resource, process);
    }

    fn detect_deadlock(&self) -> bool {
        let mut visited = BTreeSet::new();
        let mut stack = BTreeSet::new();

        for process in self.process_to_resource.keys() {
            if self.dfs(process, &mut visited, &mut stack) {
                return true;
            }
        }

        false
    }

    fn dfs(&self, process: &Process, visited: &mut BTreeSet<Process>, stack: &mut BTreeSet<Process>) -> bool {
        if stack.contains(process) {
            return true;
        }

        if visited.contains(process) {
            return false;
        }

        visited.insert(process.clone());
        stack.insert(process.clone());

        if let Some(resources) = self.process_to_resource.get(process) {
            for resource in resources {
                if let Some(next_process) = self.resource_to_process.get(resource) {
                    if self.dfs(next_process, visited, stack) {
                        return true;
                    }
                }
            }
        }

        stack.remove(process);
        false
    }
}
