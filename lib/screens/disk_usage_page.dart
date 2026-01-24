import 'package:flutter/material.dart';

class DiskUsagePage extends StatelessWidget {
  const DiskUsagePage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        leading: IconButton(icon: const Icon(Icons.menu), onPressed: () {}),
        title: const Text('Disk Usage'),
        actions: [
          IconButton(icon: const Icon(Icons.refresh), onPressed: () {}),
        ],
      ),
      body: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Headline
            const Padding(
              padding: EdgeInsets.fromLTRB(16, 24, 16, 8),
              child: Text('Total Storage Used', style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold)),
            ),
            
            // Hero Storage Card
            Padding(
              padding: const EdgeInsets.all(16.0),
              child: Card(
                elevation: 0,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: Theme.of(context).dividerColor),
                ),
                child: Padding(
                  padding: const EdgeInsets.all(24.0),
                  child: Row(
                    children: [
                      // Donut Chart Placeholder
                      SizedBox(
                        width: 120,
                        height: 120,
                        child: Stack(
                          alignment: Alignment.center,
                          children: [
                            CircularProgressIndicator(
                              value: 0.66,
                              strokeWidth: 20,
                              backgroundColor: Colors.green,
                              valueColor: AlwaysStoppedAnimation<Color>(Theme.of(context).primaryColor),
                            ),
                            Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text('USED', style: TextStyle(fontSize: 10, fontWeight: FontWeight.bold, color: Theme.of(context).hintColor)),
                                const Text('66%', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                              ],
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 24),
                      // Breakdown
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            const Text('45.2 GB Total', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                            const SizedBox(height: 8),
                            _buildLegendItem(context, '30 GB Images', Theme.of(context).primaryColor),
                            const SizedBox(height: 4),
                            _buildLegendItem(context, '15.2 GB Data', Colors.green),
                            const SizedBox(height: 16),
                            FilledButton(
                              onPressed: () {},
                              style: FilledButton.styleFrom(
                                visualDensity: VisualDensity.compact,
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
                              ),
                              child: const Text('Optimize Now'),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            
            // Section Header
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text('Containers (8)', style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
                  TextButton(onPressed: () {}, child: const Text('View All')),
                ],
              ),
            ),
            
            // Container List
            _buildContainerDiskItem(context, 'Fedora 39', '12.4 GB used', Icons.terminal, Theme.of(context).primaryColor, 0.85),
            _buildContainerDiskItem(context, 'Arch Linux', '8.1 GB used', Icons.navigation, Colors.lightBlue, 0.55),
            _buildContainerDiskItem(context, 'Ubuntu 22.04 LTS', '4.0 GB used', Icons.settings_power, Colors.orange, 0.25),
            _buildContainerDiskItem(context, 'Debian Bookworm', '2.8 GB used', Icons.radio_button_checked, Colors.red, 0.18),
            
            const SizedBox(height: 80), // Space for FAB
          ],
        ),
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: () {},
        child: const Icon(Icons.add),
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: 0,
        onDestinationSelected: (index) {},
        destinations: const [
          NavigationDestination(icon: Icon(Icons.dashboard_outlined), label: 'Usage'),
          NavigationDestination(icon: Icon(Icons.inventory_2_outlined), label: 'Containers'),
          NavigationDestination(icon: Icon(Icons.settings_outlined), label: 'Settings'),
        ],
      ),
    );
  }

  Widget _buildLegendItem(BuildContext context, String text, Color color) {
    return Row(
      children: [
        Container(width: 12, height: 12, decoration: BoxDecoration(color: color, shape: BoxShape.circle)),
        const SizedBox(width: 8),
        Text(text, style: TextStyle(color: Theme.of(context).hintColor, fontSize: 14)),
      ],
    );
  }

  Widget _buildContainerDiskItem(BuildContext context, String name, String usage, IconData icon, Color iconColor, double progress) {
    return Column(
      children: [
        ListTile(
          contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
          leading: Container(
            width: 48,
            height: 48,
            decoration: BoxDecoration(
              color: iconColor.withOpacity(0.1),
              borderRadius: BorderRadius.circular(12),
            ),
            child: Icon(icon, color: iconColor),
          ),
          title: Text(name, style: const TextStyle(fontWeight: FontWeight.bold)),
          subtitle: Text(usage),
          trailing: FilledButton.tonal(
            onPressed: () {},
            style: FilledButton.styleFrom(
              visualDensity: VisualDensity.compact,
              shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(8)),
            ),
            child: const Text('ANALYZE', style: TextStyle(fontSize: 10, fontWeight: FontWeight.bold)),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(4),
            child: LinearProgressIndicator(
              value: progress,
              backgroundColor: Theme.of(context).dividerColor.withOpacity(0.2),
              color: Theme.of(context).primaryColor,
              minHeight: 6,
            ),
          ),
        ),
        const SizedBox(height: 12),
        const Divider(height: 1),
      ],
    );
  }
}
