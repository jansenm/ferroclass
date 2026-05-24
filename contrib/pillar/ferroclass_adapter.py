# SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

"""
Ferroclass external Pillar module for Salt.

This module provides the ``ext_pillar`` interface for Salt, fetching
Pillar data from a ferroclass (reclass-compatible) inventory.

Use the "ferroclass" database as a Pillar source. This is the Rust
reimplementation of reclass, exposed via a native Python extension
module built with PyO3.

To use this plugin, add it to the ``ext_pillar`` list in the Salt
master config:

.. code-block:: yaml

    ext_pillar:
      - ferroclass:
          storage_type: yaml_fs
          inventory_base_uri: /srv/salt

If you are also using ferroclass as a ``master_tops`` plugin, and you
want to avoid having to specify the same information for both, use
YAML anchors (take note of the differing data types for ``ext_pillar``
and ``master_tops``):

.. code-block:: yaml

    ferroclass: &ferroclass
      storage_type: yaml_fs
      inventory_base_uri: /srv/salt

    ext_pillar:
      - ferroclass: *ferroclass

    master_tops:
      ferroclass: *ferroclass

Supported options (matching Python reclass adapter):

  storage_type
    Storage backend type. Default: ``yaml_fs``.
  inventory_base_uri
    Base directory for the inventory. Default: ``/etc/reclass``.
  nodes_uri
    Subdirectory for node definitions. Default: ``nodes``.
  classes_uri
    Subdirectory for class definitions. Default: ``classes``.
  compose_node_name
    Compose node names from directory paths. Default: ``false``.
  default_environment
    Default environment for nodes. Default: ``base``.
  allow_adapter_env_override
    Allow ``saltenv``/``pillarenv`` to override node environment.
    Default: ``false``.
  ignore_class_notfound
    Ignore missing classes instead of raising an error.
    Default: ``false``.
  propagate_pillar_data_to_reclass
    Pass existing pillar data into ferroclass. Not yet implemented.
    Default: ``false``.

.. note::

   The ``propagate_pillar_data_to_reclass`` option is accepted for
   compatibility with the Python reclass adapter but is currently
   ignored. Pillar propagation will be added in a future release.
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


def ext_pillar(minion_id, pillar, **kwargs):
    """
    Obtain the Pillar data from **ferroclass** for the given ``minion_id``.
    """
    import ferroclass
    from salt.exceptions import SaltInvocationError

    # Remove ferroclass-internal options that Salt shouldn't pass through.
    kwargs.pop("ferroclass_source_path", None)

    # If no inventory_base_uri was specified, initialize it to the first
    # file_roots of class 'base' (if that exists).
    if "inventory_base_uri" not in kwargs:
        _set_inventory_base_uri_default(__opts__, kwargs)

    # If saltenv or pillarenv has been set, add it to the kwargs.
    # This allows ferroclass to override a node's environment.
    env_override = None
    if __opts__.get("saltenv", None):
        env_override = __opts__["saltenv"]
    if __opts__.get("pillarenv", None):
        env_override = __opts__["pillarenv"]

    try:
        return ferroclass.ext_pillar(
            minion_id,
            pillar=pillar,
            pillarenv=env_override,
            **kwargs,
        )
    except TypeError as e:
        if "unexpected keyword argument" in str(e):
            arg = str(e).split()[-1]
            raise SaltInvocationError(
                f"ext_pillar.ferroclass: unexpected option: {arg}"
            )
        raise
    except KeyError as e:
        if "id" in str(e):
            raise SaltInvocationError(
                "ext_pillar.ferroclass: __opts__ does not define minion ID"
            )
        raise
    except Exception as e:
        raise SaltInvocationError(f"ext_pillar.ferroclass: {e}")


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