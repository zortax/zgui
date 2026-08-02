//! Fragments: several views in one place, with no wrapper element.

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::{Anchor, Empty};
use crate::view::view::View;

impl View for () {
    type State = Empty;

    fn build(self, _cx: &mut BuildCx<'_>) -> Self::State {
        Empty
    }

    fn rebuild(self, _state: &mut Self::State, _cx: &mut BuildCx<'_>) {}
}

/// Declares the view and anchor implementations for one tuple length.
macro_rules! tuple_view {
    ($( $name:ident : $state:ident ),+) => {
        #[allow(non_snake_case)]
        impl<$($name: Anchor),+> Anchor for ($($name,)+) {
            fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
                let ($($name,)+) = self;
                $( $name.mount(dom, parent, before); )+
            }

            fn unmount(&mut self, dom: &DomHandle) {
                let ($($name,)+) = self;
                $( $name.unmount(dom); )+
            }

            fn first_node(&self) -> Option<NodeId> {
                let ($($name,)+) = self;
                let mut first = None;
                $( first = first.or_else(|| $name.first_node()); )+
                first
            }
        }

        #[allow(non_snake_case)]
        impl<$($name: View),+> View for ($($name,)+) {
            type State = ($($name::State,)+);

            fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
                let ($($name,)+) = self;
                ($( $name.build(cx), )+)
            }

            fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
                let ($($name,)+) = self;
                let ($($state,)+) = state;
                $( $name.rebuild($state, cx); )+
            }
        }
    };
}

tuple_view!(A: a);
tuple_view!(A: a, B: b);
tuple_view!(A: a, B: b, C: c);
tuple_view!(A: a, B: b, C: c, D: d);
tuple_view!(A: a, B: b, C: c, D: d, E: e);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v, W: w);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v, W: w, X: x);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v, W: w, X: x, Y: y);
tuple_view!(A: a, B: b, C: c, D: d, E: e, F: f, G: g, H: h, I: i, J: j, K: k, L: l, M: m, N: n, O: o, P: p, Q: q, R: r, S: s, T: t, U: u, V: v, W: w, X: x, Y: y, Z: z);

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_interned::ElementName;
    use zgui_reactive::{Mounted, install};

    use crate::DocumentId;
    use crate::cx::BuildCxOwned;
    use crate::dom::DomHandle;
    use crate::host::HostHandle;
    use crate::stub::{StubDom, StubHost};
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn a_fragment_mounts_its_parts_in_order() {
        install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let window = Mounted::new();
        let cx = BuildCxOwned::new(
            dom.clone(),
            HostHandle::new(StubHost::default()),
            window.owner().clone(),
            DocumentId::FIRST,
        );
        let root = dom.create_element(ElementName::new("row"));

        let mut state = ("a", "b", "c").build(&mut cx.cx());
        state.mount(&dom, root, None);
        assert_eq!(backend.text_content(root), "abc");

        assert_eq!(state.first_node(), Some(state.0.node()));
        state.unmount(&dom);
        assert_eq!(backend.text_content(root), "");
        window.unmount();
    }

    #[test]
    fn an_empty_fragment_contributes_no_node() {
        install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend);
        let window = Mounted::new();
        let cx = BuildCxOwned::new(
            dom.clone(),
            HostHandle::new(StubHost::default()),
            window.owner().clone(),
            DocumentId::FIRST,
        );

        let state = ().build(&mut cx.cx());
        assert_eq!(state.first_node(), None);
        window.unmount();
    }
}
