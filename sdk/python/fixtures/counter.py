"""Production-ABI Counter: event update is visible on the next render."""

import os

from synui import (
    Abi,
    ActionCallback,
    ButtonElement,
    IntState,
    Runtime,
    TextElement,
    render,
)


def main():
    abi = Abi.load(os.environ.get("SYNUI_ABI_LIBRARY"))
    runtime = Runtime.create(abi).unwrap()
    app = runtime.create_app().unwrap()
    count = IntState.create(app, 0).unwrap()

    def increment():
        count.set(count.get().unwrap() + 1)

    callback = ActionCallback.create(app, increment).unwrap()
    text = TextElement.create(app, str(count.get().unwrap())).unwrap()
    button = ButtonElement.create(app, callback).unwrap()

    render(app, [text, button]).unwrap()
    assert text.rendered_text().unwrap() == "0"
    button.press().unwrap()
    assert count.get().unwrap() == 1
    assert text.rendered_text().unwrap() == "0"

    text.set_text(str(count.get().unwrap())).unwrap()
    assert text.rendered_text().unwrap() == "0"
    render(app, [text, button]).unwrap()
    assert text.rendered_text().unwrap() == "1"

    button.close()
    text.close()
    callback.close()
    count.close()
    app.close()
    runtime.close()


if __name__ == "__main__":
    main()
