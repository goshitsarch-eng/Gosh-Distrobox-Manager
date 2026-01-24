import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';

class ContainerTerminalPage extends StatefulWidget {
  final ContainerInfo container;

  const ContainerTerminalPage({super.key, required this.container});

  @override
  State<ContainerTerminalPage> createState() => _ContainerTerminalPageState();
}

class _ContainerTerminalPageState extends State<ContainerTerminalPage> {
  List<String>? _enterCommand;
  bool _isLoadingCommand = true;
  String? _error;

  ContainerInfo get container => widget.container;

  bool get isRunning => container.status is Status_Up;

  Color get statusColor => switch (container.status) {
        Status_Up() => Colors.green,
        Status_Created() => Colors.blue,
        Status_Exited() => Colors.orange,
        Status_Other() => Colors.grey,
      };

  String get statusText => switch (container.status) {
        Status_Up(field0: final s) => 'RUNNING${s.isNotEmpty ? ' $s' : ''}',
        Status_Created(field0: final s) => 'CREATED${s.isNotEmpty ? ' $s' : ''}',
        Status_Exited(field0: final s) => 'EXITED${s.isNotEmpty ? ' $s' : ''}',
        Status_Other(field0: final s) => s.isNotEmpty ? s.toUpperCase() : 'UNKNOWN',
      };

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    if (imageLower.contains('centos') || imageLower.contains('rocky')) {
      return Icons.shield;
    }
    if (imageLower.contains('opensuse') || imageLower.contains('suse')) {
      return Icons.pets;
    }
    return Icons.dns;
  }

  @override
  void initState() {
    super.initState();
    _loadEnterCommand();
  }

  Future<void> _loadEnterCommand() async {
    setState(() {
      _isLoadingCommand = true;
      _error = null;
    });

    try {
      final appState = context.read<AppStateProvider>();
      final command = await appState.getEnterCommand(container.name);
      if (mounted) {
        setState(() {
          _enterCommand = command;
          _isLoadingCommand = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _error = e.toString();
          _isLoadingCommand = false;
        });
      }
    }
  }

  void _copyCommand() {
    if (_enterCommand != null) {
      final commandStr = _enterCommand!.join(' ');
      Clipboard.setData(ClipboardData(text: commandStr));
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Command copied to clipboard'),
          duration: Duration(seconds: 2),
        ),
      );
    }
  }

  Future<void> _stopContainer() async {
    final appState = context.read<AppStateProvider>();
    final success = await appState.stopContainer(container.name);
    if (success && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Container stopped')),
      );
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(container.name),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: _loadEnterCommand,
          ),
        ],
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Container Info Header
            Row(
              children: [
                Container(
                  width: 48,
                  height: 48,
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Icon(
                    _getDistroIcon(container.image),
                    color: Theme.of(context).colorScheme.primary,
                  ),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Text(
                            container.name,
                            style: const TextStyle(
                              fontSize: 20,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                          const SizedBox(width: 8),
                          Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 8, vertical: 2),
                            decoration: BoxDecoration(
                              color: statusColor.withOpacity(0.1),
                              borderRadius: BorderRadius.circular(12),
                              border: Border.all(color: statusColor.withOpacity(0.5)),
                            ),
                            child: Text(
                              statusText,
                              style: TextStyle(
                                fontSize: 10,
                                fontWeight: FontWeight.bold,
                                color: statusColor,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      Text(
                        container.image,
                        style: TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          color: Theme.of(context).hintColor,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),
              ],
            ),

            const SizedBox(height: 24),

            // Terminal Command Section
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Icon(
                          Icons.terminal,
                          color: Theme.of(context).colorScheme.primary,
                        ),
                        const SizedBox(width: 8),
                        const Text(
                          'Terminal Access',
                          style: TextStyle(
                            fontWeight: FontWeight.bold,
                            fontSize: 16,
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    if (!isRunning) ...[
                      Container(
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: Colors.orange.withOpacity(0.1),
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Row(
                          children: [
                            const Icon(Icons.warning_amber, color: Colors.orange),
                            const SizedBox(width: 12),
                            Expanded(
                              child: Text(
                                'Container is not running. Start the container to access the terminal.',
                                style: TextStyle(color: Colors.orange[800]),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ] else if (_isLoadingCommand) ...[
                      const Center(child: CircularProgressIndicator()),
                    ] else if (_error != null) ...[
                      Container(
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: Colors.red.withOpacity(0.1),
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Row(
                          children: [
                            const Icon(Icons.error_outline, color: Colors.red),
                            const SizedBox(width: 12),
                            Expanded(
                              child: Text(
                                'Error: $_error',
                                style: const TextStyle(color: Colors.red),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ] else ...[
                      const Text(
                        'Run this command in your terminal to enter the container:',
                        style: TextStyle(fontSize: 14),
                      ),
                      const SizedBox(height: 12),
                      Container(
                        width: double.infinity,
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          color: const Color(0xFF1E1E1E),
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Row(
                          children: [
                            Expanded(
                              child: SelectableText(
                                _enterCommand?.join(' ') ?? '',
                                style: const TextStyle(
                                  fontFamily: 'monospace',
                                  fontSize: 14,
                                  color: Colors.greenAccent,
                                ),
                              ),
                            ),
                            IconButton(
                              icon: const Icon(Icons.copy, color: Colors.white70),
                              tooltip: 'Copy command',
                              onPressed: _copyCommand,
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 16),
                      SizedBox(
                        width: double.infinity,
                        child: FilledButton.icon(
                          onPressed: _copyCommand,
                          icon: const Icon(Icons.content_copy),
                          label: const Text('Copy Command'),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ),

            const SizedBox(height: 16),

            // Quick Actions
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      'Quick Actions',
                      style: TextStyle(
                        fontWeight: FontWeight.bold,
                        fontSize: 16,
                      ),
                    ),
                    const SizedBox(height: 16),
                    Row(
                      children: [
                        Expanded(
                          child: _buildActionButton(
                            context,
                            Icons.content_paste,
                            'Copy\nCommand',
                            Colors.blue,
                            isRunning ? _copyCommand : null,
                          ),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: _buildActionButton(
                            context,
                            Icons.system_update_alt,
                            'Upgrade\nPackages',
                            Colors.purple,
                            isRunning
                                ? () async {
                                    final appState =
                                        context.read<AppStateProvider>();
                                    await appState
                                        .upgradeContainer(container.name);
                                    if (mounted) {
                                      ScaffoldMessenger.of(context).showSnackBar(
                                        const SnackBar(
                                            content:
                                                Text('Upgrade started...')),
                                      );
                                    }
                                  }
                                : null,
                          ),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: _buildActionButton(
                            context,
                            Icons.stop,
                            'Stop\nContainer',
                            Colors.red,
                            isRunning ? _stopContainer : null,
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),

            const SizedBox(height: 16),

            // Container Details
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    const Text(
                      'Container Details',
                      style: TextStyle(
                        fontWeight: FontWeight.bold,
                        fontSize: 16,
                      ),
                    ),
                    const SizedBox(height: 16),
                    _buildDetailRow('Container ID', container.id),
                    const Divider(),
                    _buildDetailRow('Name', container.name),
                    const Divider(),
                    _buildDetailRow('Image', container.image),
                    const Divider(),
                    _buildDetailRow('Status', statusText),
                  ],
                ),
              ),
            ),

            const SizedBox(height: 16),

            // Help Text
            Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.primary.withOpacity(0.05),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: Theme.of(context).colorScheme.primary.withOpacity(0.2),
                ),
              ),
              child: Row(
                children: [
                  Icon(
                    Icons.info_outline,
                    color: Theme.of(context).colorScheme.primary,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      'Gosh Distrobox Manager currently provides the command to enter containers. Full terminal emulation is planned for a future release.',
                      style: TextStyle(
                        fontSize: 13,
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildActionButton(
    BuildContext context,
    IconData icon,
    String label,
    Color color,
    VoidCallback? onPressed,
  ) {
    final isEnabled = onPressed != null;
    return InkWell(
      onTap: onPressed,
      borderRadius: BorderRadius.circular(12),
      child: Container(
        height: 80,
        decoration: BoxDecoration(
          color: isEnabled
              ? color.withOpacity(0.1)
              : Theme.of(context).disabledColor.withOpacity(0.05),
          borderRadius: BorderRadius.circular(12),
          border: Border.all(
            color: isEnabled
                ? color.withOpacity(0.3)
                : Theme.of(context).disabledColor.withOpacity(0.1),
          ),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              icon,
              color: isEnabled ? color : Theme.of(context).disabledColor,
            ),
            const SizedBox(height: 4),
            Text(
              label,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.bold,
                color: isEnabled ? color : Theme.of(context).disabledColor,
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildDetailRow(String label, String value) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 100,
            child: Text(
              label,
              style: TextStyle(
                fontWeight: FontWeight.w500,
                color: Theme.of(context).hintColor,
              ),
            ),
          ),
          Expanded(
            child: SelectableText(
              value,
              style: const TextStyle(fontFamily: 'monospace'),
            ),
          ),
        ],
      ),
    );
  }
}
