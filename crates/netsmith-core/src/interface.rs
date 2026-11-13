//! Interface instances and the nets their members are on.

/// One member of an interface instance and the net it is on: `tx` on `UART0_TX`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterfaceMember {
    /// The member's name in its interface: `tx`.
    pub name: String,
    /// The name of the net the member is on, one of [`Design::nets`].
    pub net: String,
}

/// An interface instance of the design and the nets its members are on, as the checker's
/// model has them: the instance a module or a part declares (`.uart0: Uart`), not a guess
/// from net names. A schematic reads it to draw the members of one interface as one bus.
/// Only the pins that are directly in the interface are members (a rail inside a `Usb2` is
/// an interface instance of its own), only members that are on a net of the design are
/// listed, and an instance with none is left out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterfaceInstance {
    /// Where the instance is in the design, the path of its instance from the board:
    /// `mcu.uart0`. It does not change when reference designators are renumbered.
    pub path: String,
    /// The interface definition it was made from: `Uart`.
    pub type_name: String,
    /// The members that are on nets, in the order the interface declares them.
    pub members: Vec<InterfaceMember>,
}
