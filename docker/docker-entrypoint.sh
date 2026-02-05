#!/bin/bash
set -e

# Start ZeroTier in the background
zerotier-one -d

# Wait for ZeroTier to be ready
echo "Waiting for ZeroTier to start..."
until [ -f /var/lib/zerotier-one/authtoken.secret ]; do
    sleep 1
done
echo "ZeroTier started"
echo ""
echo "=========================================="
echo "ZeroTier Auth Token (for TierDrop setup):"
echo "$(cat /var/lib/zerotier-one/authtoken.secret)"
echo "=========================================="
echo ""

# Start TierDrop (bind to all interfaces for container networking)
export TIERDROP_BIND="0.0.0.0:8000"
exec tierdrop
