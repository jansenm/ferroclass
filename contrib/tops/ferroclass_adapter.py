# SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

"""
Ferroclass master_tops module for Salt.

This module provides the ``master_tops`` interface for Salt, reading
top data (environment → minion → state list) from a ferroclass
(reclass-compatible) inventory.

To use this plugin, add it to the ``master_tops`` list in the Salt
master config:

.. code-block:: yaml

    master_tops:
      ferroclass:
        storage_type: yaml_fs
        inventory_base_uri: /srv/salt

See the pillar adapter documentation for a full list of supported
options, including ``class_mappings`` and ``class_mappings_match_path``.
"""

__virtualname__ = "ferroclass"


def __virtual__():
    """
    Only load the module if the ferroclass Python package is available.
    """
    try:
        import ferroclass  # noqa: F401
        return __virtualname__
    except ImportError:
        return False


def top(**kwargs):
    """
    Query **ferroclass** for the top data (states of the minions).
    """
    import ferroclass
    from salt.exceptions import SaltInvocationError

    # Salt's top interface is inconsistent with ext_pillar (see Salt #5786).
    # One is expected to extract the arguments from the master_tops config.
    reclass_opts = __opts__.get("master_tops", {}).get("ferroclass", {})

    # Remove ferroclass-internal options that Salt shouldn't pass through.
    reclass_opts.pop("ferroclass_source_path", None)

    # If no inventory_base_uri was specified, initialize it to the first
    # file_roots of class 'base' (if that exists).
    if "inventory_base_uri" not in reclass_opts:
        _set_inventory_base_uri_default(__opts__, reclass_opts)

    # Salt expects the top data to be filtered by minion_id, so we need
    # to extract it from the kwargs (see Salt #6930).
    minion_id = kwargs["opts"]["id"]

    # If saltenv or pillarenv has been set, add it to the kwargs.
    # This allows ferroclass to override a node's environment.
    env_override = None
    if kwargs["opts"].get("saltenv", None):
        env_override = kwargs["opts"]["saltenv"]
    if kwargs["opts"].get("pillarenv", None):
        env_override = kwargs["opts"]["pillarenv"]

    try:
        return ferroclass.top(
            minion_id=minion_id,
            pillarenv=env_override,
            **reclass_opts,
        )
    except TypeError as e:
        if "unexpected keyword argument" in str(e):
            arg = str(e).split()[-1]
            raise SaltInvocationError(
                f"master_tops.ferroclass: unexpected option: {arg}"
            )
        raise
    except KeyError as e:
        if "id" in str(e):
            raise SaltInvocationError(
                "master_tops.ferroclass: __opts__ does not define minion ID"
            )
        raise
    except Exception as e:
        raise SaltInvocationError(f"master_tops.ferroclass: {e}")


def _set_inventory_base_uri_default(opts, kwargs):
    """
    If inventory_base_uri is not set, default to the first file_roots
    entry of the 'base' environment, matching the Python reclass adapter.
    """
    try:
        file_roots = opts.get("file_roots", {})
        base_roots = file_roots.get("base", [])
        if base_roots:
            kwargs["inventory_base_uri"] = base_roots[0]
    except (TypeError, AttributeError, IndexError):
        pass