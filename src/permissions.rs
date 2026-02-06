use crate::state::User;

/// Check if user can read a network (view details and members)
pub fn can_read(user: &User, nwid: &str) -> bool {
    if user.is_admin {
        return true;
    }
    user.get_network_permissions(nwid).read
}

/// Check if user can authorize/deauthorize members
pub fn can_authorize(user: &User, nwid: &str) -> bool {
    if user.is_admin {
        return true;
    }
    user.get_network_permissions(nwid).authorize
}

/// Check if user can modify network settings (IP pools, routes, DNS, etc.)
pub fn can_modify(user: &User, nwid: &str) -> bool {
    if user.is_admin {
        return true;
    }
    user.get_network_permissions(nwid).modify
}

/// Check if user can delete the network itself
pub fn can_delete(user: &User, nwid: &str) -> bool {
    if user.is_admin {
        return true;
    }
    user.get_network_permissions(nwid).delete
}

/// Check if user is an admin (can manage users, create networks, etc.)
pub fn is_admin(user: &User) -> bool {
    user.is_admin
}

/// Check if user has any permission on a network
pub fn has_any_permission(user: &User, nwid: &str) -> bool {
    if user.is_admin {
        return true;
    }
    user.get_network_permissions(nwid).has_any()
}
