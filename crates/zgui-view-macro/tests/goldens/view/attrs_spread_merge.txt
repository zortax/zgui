# a whole class list on a component is a prop, not a forwarded entry
class list: root w-full
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
a11y role=Button label=Some("component")

# class and style toggles compose, and the caller is last
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
class-toggle busy = true
style gap = Some("2rem")
a11y role=Button label=Some("component")

# the caller's accessibility properties win
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
a11y role=Link label=Some("caller")

# a caller who named no role does not take the component's away
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
a11y role=Button label=Some("caller")

# listeners accumulate, the component's first
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
listener click capture=true
a11y role=Button label=Some("component")

# attributes, custom properties and properties are last-write-wins, caller last
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
attribute data-part = Some("caller")
custom-property brand = Some("caller")
property value = Text("caller")
a11y role=Button label=Some("component")

# a state a view may assert, and one it defines itself
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
state UiState(DISABLED) = true
custom-state selected = true
a11y role=Button label=Some("component")

# a forwarded bundle is replayed where the spread was written
class list: root
class-toggle busy = false
style gap = Some("1rem")
custom-property brand = Some("component")
attribute data-part = Some("root")
property value = Text("own")
listener click capture=false
class-toggle before = true
class-toggle forwarded = true
attribute data-source = Some("bundle")
class-toggle after = true
a11y role=Button label=Some("component")
