//! Todo list using the v0.2 facade with a real `Scroll` host.
//!
//! File-menu import remains a platform boundary (N/A this phase).

use synui::prelude::*;

#[derive(Clone, PartialEq)]
struct TodoItem {
    id: u64,
    text: String,
    done: bool,
}

fn main() -> Result<(), PlatformError> {
    let app = App::new(AppConfig::default())?;
    let context = app.context();
    let next_id = context.state(2_u64)?;
    let tasks = context.state(vec![TodoItem {
        id: 1,
        text: "Learn SynUI".into(),
        done: false,
    }])?;
    let draft = context.state(String::new())?;
    let error = context.state(String::new())?;

    let update_draft = context.action({
        let draft = draft.clone();
        move |_, value: String| draft.set(value).expect("draft state is active")
    })?;
    let add = context.action({
        let tasks = tasks.clone();
        let draft = draft.clone();
        let error = error.clone();
        let next_id = next_id.clone();
        move |_, (): ()| {
            let value = draft.get().expect("draft state is active");
            let text = value.trim().to_owned();
            if text.is_empty() {
                error
                    .set("A task title is required".into())
                    .expect("error state is active");
                return;
            }
            let id = next_id.get().expect("id state is active");
            next_id.set(id + 1).expect("id state is active");
            let mut current = tasks.get().expect("tasks state is active");
            current.push(TodoItem {
                id,
                text,
                done: false,
            });
            tasks.set(current).expect("tasks state is active");
            draft.set(String::new()).expect("draft state is active");
            error.set(String::new()).expect("error state is active");
        }
    })?;
    let toggle = context.action({
        let tasks = tasks.clone();
        move |_, id: u64| {
            let mut current = tasks.get().expect("tasks state is active");
            if let Some(item) = current.iter_mut().find(|item| item.id == id) {
                item.done = !item.done;
            }
            tasks.set(current).expect("tasks state is active");
        }
    })?;
    let delete = context.action({
        let tasks = tasks.clone();
        move |_, id: u64| {
            let mut current = tasks.get().expect("tasks state is active");
            current.retain(|item| item.id != id);
            tasks.set(current).expect("tasks state is active");
        }
    })?;

    app.run(Window::new("Todo List"), move |_| {
        let items = tasks.get()?;
        let current_error = error.get()?;
        let rows = items.into_iter().map(|item| {
            let id = item.id;
            let toggle = toggle.clone();
            let delete = delete.clone();
            let label = if item.done {
                format!("[x] {}", item.text)
            } else {
                format!("[ ] {}", item.text)
            };
            Node::row([
                Node::text(label).key(BusinessKey::new(format!("label-{id}"))),
                Button::new("Toggle")
                    .on_click(move |event: EventContext| {
                        let _ = toggle.invoke(event.app(), id);
                    })
                    .node()
                    .key(BusinessKey::new(format!("toggle-{id}"))),
                Button::new("Delete")
                    .on_click(move |event: EventContext| {
                        let _ = delete.invoke(event.app(), id);
                    })
                    .node()
                    .key(BusinessKey::new(format!("delete-{id}"))),
            ])
            .key(BusinessKey::unsigned(id))
        });

        Ok(Node::column([
            Node::text("Todo List").key(BusinessKey::new("title")),
            TextInput::new("New task")
                .controlled(draft.clone(), update_draft.clone())
                .node()
                .key(BusinessKey::new("draft")),
            Button::new("Add").on_press(add.clone()).node(),
            if current_error.is_empty() {
                Node::text("Add a task, then Toggle / Delete on its row")
            } else {
                Node::text(format!("Error: {current_error}")).key(BusinessKey::new("error"))
            },
            Scroll::new(rows).node().key(BusinessKey::new("tasks")),
        ]))
    })
}
